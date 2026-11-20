# Secrets placeholders created in AWS Secrets Manager.
# Populate secret values out-of-band (AWS CLI / Console / external secret operator)
# so raw credentials never enter Terraform state.

resource "aws_secretsmanager_secret" "google_oidc_client_secret" {
  name        = "${local.name_prefix}/google-oidc-client-secret"
  description = "Google Workspace OIDC Client Secret for afterlifed OAuth 2.1 broker"
}

resource "aws_secretsmanager_secret" "jwt_signing_key" {
  name        = "${local.name_prefix}/jwt-signing-key"
  description = "HS256/EdDSA signing key used by afterlifed to mint short-lived access tokens"
}

resource "aws_secretsmanager_secret" "database_url" {
  name        = "${local.name_prefix}/database-url"
  description = "PostgreSQL connection DSN using the least-privilege RLS-subject application role"
}
