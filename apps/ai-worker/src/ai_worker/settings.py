from urllib.parse import quote

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    aws_bearer_token_bedrock: str = Field(default="", alias="AWS_BEARER_TOKEN_BEDROCK")
    aws_region: str = Field(default="us-east-1", alias="AWS_REGION")
    bedrock_model_id: str = Field(
        default="us.anthropic.claude-sonnet-4-6",
        alias="BEDROCK_MODEL_ID",
    )

    @property
    def invoke_url(self) -> str:
        model = quote(self.bedrock_model_id, safe="")
        return (
            f"https://bedrock-runtime.{self.aws_region}.amazonaws.com"
            f"/model/{model}/converse"
        )
