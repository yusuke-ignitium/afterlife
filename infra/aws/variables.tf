variable "aws_region" {
  description = "AWS region for the Afterlife control plane"
  type        = string
  default     = "ap-northeast-1"
}

variable "environment" {
  description = "Deployment environment label (e.g. prod, staging)"
  type        = string
  default     = "prod"
}

variable "vpc_cidr" {
  description = "VPC CIDR block"
  type        = string
  default     = "10.42.0.0/16"
}

variable "public_base_url" {
  description = "Public HTTPS origin for OAuth 2.1 issuer, REST, SSE, and Streamable HTTP MCP (/mcp)"
  type        = string
  default     = "https://afterlife.example.com"
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN for HTTPS listener on the ALB"
  type        = string
}

variable "google_workspace_hosted_domain" {
  description = "Google Workspace domain allowed to authenticate (checked against OIDC `hd` claim)"
  type        = string
}

variable "google_oidc_client_id" {
  description = "Google Workspace OIDC Client ID"
  type        = string
}

variable "ecs_desired_count" {
  description = "Number of ARM64 Fargate tasks behind the ALB (2 for zero-downtime deploys & HA)"
  type        = number
  default     = 2
}

variable "ecs_cpu" {
  description = "Fargate task CPU units (512 = 0.5 vCPU)"
  type        = number
  default     = 512
}

variable "ecs_memory" {
  description = "Fargate task memory in MiB"
  type        = number
  default     = 1024
}

variable "container_image_tag" {
  description = "Image tag in the Afterlife ECR repository"
  type        = string
  default     = "latest"
}

variable "db_instance_class" {
  description = "RDS PostgreSQL 16 Graviton instance class (sized for 30-100 engineers)"
  type        = string
  default     = "db.t4g.small"
}

variable "db_multi_az" {
  description = "Enable Multi-AZ standby for RDS PostgreSQL (false saves ~$25/mo; enable for strict HA)"
  type        = bool
  default     = false
}

variable "db_allocated_storage_gb" {
  description = "Initial gp3 storage allocation for RDS PostgreSQL"
  type        = number
  default     = 20
}

variable "db_max_allocated_storage_gb" {
  description = "Maximum autoscaled storage for RDS PostgreSQL"
  type        = number
  default     = 100
}

variable "db_master_username" {
  description = "RDS PostgreSQL master username"
  type        = string
  default     = "afterlife_admin"
}

variable "db_master_password" {
  description = "Initial RDS master password (prefer rotating via Secrets Manager after bootstrap)"
  type        = string
  sensitive   = true
}

variable "analytics_reader_principal_arns" {
  description = "Optional AWS IAM role ARNs granted read-only access to the append-only S3 telemetry bucket (Athena/QuickSight/Lake Formation). Afterlife itself never manages telemetry visibility."
  type        = list(string)
  default     = []
}
