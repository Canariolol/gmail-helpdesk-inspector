FROM python:3.14-slim
WORKDIR /app
COPY apps/ai-worker/pyproject.toml pyproject.toml
RUN pip install --no-cache-dir ".[test]"
COPY apps/ai-worker/src src
ENV PYTHONPATH=/app/src
EXPOSE 8090
CMD ["uvicorn", "ai_worker.main:app", "--host", "0.0.0.0", "--port", "8090"]
