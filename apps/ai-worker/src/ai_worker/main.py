from fastapi import FastAPI, HTTPException

from ai_worker.bedrock import audit_with_bedrock
from ai_worker.schemas import AuditThreadRequest, AuditThreadResponse
from ai_worker.settings import Settings

app = FastAPI(title="Gmail Helpdesk AI Worker")
settings = Settings()


@app.get("/health")
async def health() -> dict[str, bool]:
    return {"ok": True}


@app.post("/audit/thread", response_model=AuditThreadResponse)
async def audit_thread(payload: AuditThreadRequest) -> AuditThreadResponse:
    try:
        return await audit_with_bedrock(payload, settings)
    except Exception as exc:
        raise HTTPException(status_code=502, detail=str(exc)) from exc

