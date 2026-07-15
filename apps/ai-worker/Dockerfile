FROM python:3.14-slim
COPY --from=ghcr.io/astral-sh/uv:0.11.3 /uv /uvx /bin/
WORKDIR /app
COPY apps/ai-worker/pyproject.toml apps/ai-worker/uv.lock ./
COPY apps/ai-worker/src src
RUN uv sync --frozen --no-dev \
 && rm -rf /root/.cache/uv /uv /uvx \
 && useradd --system --create-home --uid 10001 --home-dir /app app \
 && chown -R app:app /app
ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONPATH=/app/src
USER app
EXPOSE 8090
CMD ["sh", "-c", "uvicorn ai_worker.main:app --host 0.0.0.0 --port ${PORT:-8090}"]
