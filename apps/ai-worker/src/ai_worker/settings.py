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
    bedrock_batch_model_id: str = Field(default="", alias="BEDROCK_BATCH_MODEL_ID")

    # Legacy fallback context. Production requests should send policy_context
    # per analysis run; these defaults intentionally contain no tenant/company data.
    desk_mailbox: str = Field(default="", alias="DESK_MAILBOX")
    analyzed_mailbox: str = Field(default="", alias="ANALYZED_MAILBOX")
    internal_domain: str = Field(default="", alias="INTERNAL_DOMAIN")
    desk_members: str = Field(default="", alias="DESK_MEMBERS")

    @property
    def invoke_url(self) -> str:
        model = quote(self.bedrock_model_id, safe="")
        return (
            f"https://bedrock-runtime.{self.aws_region}.amazonaws.com"
            f"/model/{model}/converse"
        )

    @property
    def batch_invoke_url(self) -> str:
        model = quote(self.bedrock_batch_model_id or self.bedrock_model_id, safe="")
        return (
            f"https://bedrock-runtime.{self.aws_region}.amazonaws.com"
            f"/model/{model}/converse"
        )
