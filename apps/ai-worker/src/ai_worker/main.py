from contextlib import asynccontextmanager
import json
import logging
import sys
import time
from uuid import UUID, uuid4

import httpx
from fastapi import FastAPI, HTTPException, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse

from ai_worker.bedrock import audit_batch_with_bedrock, audit_with_bedrock
from ai_worker.schemas import (
    AuditThreadRequest,
    AuditThreadResponse,
    BatchAuditRequest,
    BatchAuditResponse,
)
from ai_worker.settings import Settings

settings = Settings()


class JsonFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        event = {
            "severity": record.levelname,
            "message": record.getMessage(),
            "service": "ai-worker",
            "environment": settings.app_env,
        }
        for field in (
            "request_id",
            "operation",
            "status",
            "duration_ms",
            "run_id",
            "error_code",
        ):
            value = getattr(record, field, None)
            if value is not None:
                event[field] = value
        return json.dumps(event, ensure_ascii=False)


logger = logging.getLogger("ai_worker")
logger.setLevel(logging.INFO)
logger.propagate = False
if not logger.handlers:
    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(JsonFormatter())
    logger.addHandler(handler)


@asynccontextmanager
async def lifespan(app: FastAPI):
    # Reuse one pooled client for the whole process so every Bedrock inference
    # does not pay a fresh TLS handshake / connection setup.
    async with httpx.AsyncClient(timeout=httpx.Timeout(60.0, connect=10.0)) as client:
        app.state.bedrock_client = client
        logger.info("worker started", extra={"operation": "startup"})
        yield


app = FastAPI(title="Gmail Helpdesk AI Worker", lifespan=lifespan)


def correlated_request_id(value: str | None) -> str:
    try:
        return str(UUID(value)) if value else str(uuid4())
    except ValueError:
        return str(uuid4())


@app.middleware("http")
async def log_request(request: Request, call_next):
    request_id = correlated_request_id(request.headers.get("x-request-id"))
    started_at = time.perf_counter()
    try:
        response = await call_next(request)
    except Exception:
        logger.error(
            "worker request failed",
            extra={
                "request_id": request_id,
                "operation": request.url.path,
                "status": 500,
                "duration_ms": round((time.perf_counter() - started_at) * 1000),
                "error_code": "worker_request_failed",
            },
        )
        raise
    response.headers["x-request-id"] = request_id
    logger.info(
        "worker request completed",
        extra={
            "request_id": request_id,
            "operation": request.url.path,
            "status": response.status_code,
            "duration_ms": round((time.perf_counter() - started_at) * 1000),
        },
    )
    return response


@app.exception_handler(RequestValidationError)
async def invalid_request(_request: Request, _exc: RequestValidationError) -> JSONResponse:
    return JSONResponse(status_code=422, content={"detail": "Invalid audit request"})


@app.get("/health")
async def health() -> dict[str, bool]:
    return {"ok": True}


@app.post("/audit/thread", response_model=AuditThreadResponse)
async def audit_thread(payload: AuditThreadRequest, request: Request) -> AuditThreadResponse:
    try:
        return await audit_with_bedrock(payload, settings, request.app.state.bedrock_client)
    except Exception:
        logger.error(
            "Bedrock audit failed",
            extra={
                "operation": "audit_thread",
                "run_id": payload.thread.analysis_run_id,
                "error_code": "bedrock_failed",
            },
        )
        raise HTTPException(status_code=502, detail="AI worker unavailable") from None


@app.post("/audit/batch", response_model=BatchAuditResponse)
async def audit_batch(payload: BatchAuditRequest, request: Request) -> BatchAuditResponse:
    try:
        return await audit_batch_with_bedrock(
            payload, settings, request.app.state.bedrock_client
        )
    except Exception:
        logger.error(
            "Bedrock batch audit failed",
            extra={
                "operation": "audit_batch",
                "error_code": "bedrock_failed",
            },
        )
        raise HTTPException(status_code=502, detail="AI worker unavailable") from None
