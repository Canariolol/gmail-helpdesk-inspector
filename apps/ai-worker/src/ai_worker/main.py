from contextlib import asynccontextmanager

import httpx
from fastapi import FastAPI, HTTPException, Request

from ai_worker.bedrock import audit_batch_with_bedrock, audit_with_bedrock
from ai_worker.schemas import (
    AuditThreadRequest,
    AuditThreadResponse,
    BatchAuditRequest,
    BatchAuditResponse,
)
from ai_worker.settings import Settings

settings = Settings()


@asynccontextmanager
async def lifespan(app: FastAPI):
    # Reuse one pooled client for the whole process so every Bedrock inference
    # does not pay a fresh TLS handshake / connection setup.
    async with httpx.AsyncClient(timeout=httpx.Timeout(60.0, connect=10.0)) as client:
        app.state.bedrock_client = client
        yield


app = FastAPI(title="Gmail Helpdesk AI Worker", lifespan=lifespan)


@app.get("/health")
async def health() -> dict[str, bool]:
    return {"ok": True}


@app.post("/audit/thread", response_model=AuditThreadResponse)
async def audit_thread(payload: AuditThreadRequest, request: Request) -> AuditThreadResponse:
    try:
        return await audit_with_bedrock(payload, settings, request.app.state.bedrock_client)
    except Exception as exc:
        raise HTTPException(status_code=502, detail=str(exc)) from exc


@app.post("/audit/batch", response_model=BatchAuditResponse)
async def audit_batch(payload: BatchAuditRequest, request: Request) -> BatchAuditResponse:
    try:
        return await audit_batch_with_bedrock(
            payload, settings, request.app.state.bedrock_client
        )
    except Exception as exc:
        raise HTTPException(status_code=502, detail=str(exc)) from exc
