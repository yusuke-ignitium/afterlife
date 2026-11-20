data "aws_caller_identity" "current" {}

# 1. Operational Artifact Blob Store (owner-isolated by S3 prefix + application checks)
resource "aws_s3_bucket" "artifacts" {
  bucket = "${local.name_prefix}-artifacts-${data.aws_caller_identity.current.account_id}"
}

resource "aws_s3_bucket_public_access_block" "artifacts" {
  bucket                  = aws_s3_bucket.artifacts.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_versioning" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id

  versioning_configuration {
    status = "Enabled"
  }
}

# 2. Append-Only Hive-Partitioned Telemetry Bucket (`events/dt=YYYY-MM-DD/owner=<id>/events.jsonl`)
# Afterlife ONLY writes (`s3:PutObject`). Downstream analytics visibility (Athena, QuickSight,
# Lake Formation, org-level dashboards) is governed strictly via AWS IAM outside Afterlife.
resource "aws_s3_bucket" "telemetry" {
  bucket = "${local.name_prefix}-telemetry-${data.aws_caller_identity.current.account_id}"
}

resource "aws_s3_bucket_public_access_block" "telemetry" {
  bucket                  = aws_s3_bucket.telemetry.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "telemetry" {
  bucket = aws_s3_bucket.telemetry.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "telemetry" {
  bucket = aws_s3_bucket.telemetry.id

  rule {
    id     = "tier-cold-telemetry"
    status = "Enabled"

    transition {
      days          = 90
      storage_class = "STANDARD_IA"
    }

    transition {
      days          = 365
      storage_class = "GLACIER_IR"
    }
  }
}

data "aws_iam_policy_document" "telemetry_readers" {
  count = length(var.analytics_reader_principal_arns) > 0 ? 1 : 0

  statement {
    sid    = "AllowConfiguredAnalyticsReaders"
    effect = "Allow"

    principals {
      type        = "AWS"
      identifiers = var.analytics_reader_principal_arns
    }

    actions = [
      "s3:GetObject",
      "s3:ListBucket",
    ]

    resources = [
      aws_s3_bucket.telemetry.arn,
      "${aws_s3_bucket.telemetry.arn}/*",
    ]
  }
}

resource "aws_s3_bucket_policy" "telemetry" {
  count  = length(var.analytics_reader_principal_arns) > 0 ? 1 : 0
  bucket = aws_s3_bucket.telemetry.id
  policy = data.aws_iam_policy_document.telemetry_readers[0].json
}
