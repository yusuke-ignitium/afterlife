output "alb_dns_name" {
  description = "ALB DNS name (point your Route 53 ALIAS record here)"
  value       = aws_lb.main.dns_name
}

output "mcp_endpoint_url" {
  description = "Streamable HTTP MCP endpoint URL for Claude / Codex / Cursor / Grok clients"
  value       = "${var.public_base_url}/mcp"
}

output "ecr_repository_url" {
  description = "ECR repository URL for pushing ARM64 afterlifed images"
  value       = aws_ecr_repository.afterlifed.repository_url
}

output "rds_endpoint" {
  description = "Private RDS PostgreSQL 16 endpoint"
  value       = aws_db_instance.main.address
}

output "artifacts_bucket" {
  description = "S3 bucket name for operational job artifacts"
  value       = aws_s3_bucket.artifacts.id
}

output "telemetry_bucket" {
  description = "S3 bucket name for append-only Hive-partitioned telemetry JSONL"
  value       = aws_s3_bucket.telemetry.id
}
