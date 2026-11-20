resource "aws_ecr_repository" "afterlifed" {
  name                 = "${local.name_prefix}-daemon"
  image_tag_mutability = "IMMUTABLE"

  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_cloudwatch_log_group" "afterlifed" {
  name              = "/ecs/${local.name_prefix}"
  retention_in_days = 30
}

resource "aws_ecs_cluster" "main" {
  name = "${local.name_prefix}-cluster"

  setting {
    name  = "containerInsights"
    value = "disabled"
  }
}

data "aws_iam_policy_document" "ecs_assume" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "ecs_execution" {
  name               = "${local.name_prefix}-ecs-exec-role"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume.json
}

resource "aws_iam_role_policy_attachment" "ecs_execution_base" {
  role       = aws_iam_role.ecs_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

data "aws_iam_policy_document" "ecs_execution_secrets" {
  statement {
    effect = "Allow"
    actions = [
      "secretsmanager:GetSecretValue",
    ]
    resources = [
      aws_secretsmanager_secret.google_oidc_client_secret.arn,
      aws_secretsmanager_secret.jwt_signing_key.arn,
      aws_secretsmanager_secret.database_url.arn,
    ]
  }
}

resource "aws_iam_role_policy" "ecs_execution_secrets" {
  name   = "${local.name_prefix}-read-secrets"
  role   = aws_iam_role.ecs_execution.id
  policy = data.aws_iam_policy_document.ecs_execution_secrets.json
}

resource "aws_iam_role" "ecs_task" {
  name               = "${local.name_prefix}-ecs-task-role"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume.json
}

# Least-privilege S3 policy:
# - Artifacts bucket: read/write operational job blobs.
# - Telemetry bucket: WRITE-ONLY (`s3:PutObject`). Control plane never reads back analytics data.
data "aws_iam_policy_document" "ecs_task_s3" {
  statement {
    sid    = "ArtifactsReadWrite"
    effect = "Allow"
    actions = [
      "s3:GetObject",
      "s3:PutObject",
    ]
    resources = [
      "${aws_s3_bucket.artifacts.arn}/*",
    ]
  }

  statement {
    sid    = "TelemetryAppendOnlyWrite"
    effect = "Allow"
    actions = [
      "s3:PutObject",
    ]
    resources = [
      "${aws_s3_bucket.telemetry.arn}/*",
    ]
  }
}

resource "aws_iam_role_policy" "ecs_task_s3" {
  name   = "${local.name_prefix}-s3-access"
  role   = aws_iam_role.ecs_task.id
  policy = data.aws_iam_policy_document.ecs_task_s3.json
}

resource "aws_ecs_task_definition" "afterlifed" {
  family                   = "${local.name_prefix}-daemon"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.ecs_cpu
  memory                   = var.ecs_memory
  execution_role_arn       = aws_iam_role.ecs_execution.arn
  task_role_arn            = aws_iam_role.ecs_task.arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "ARM64"
  }

  container_definitions = jsonencode([
    {
      name      = "afterlifed"
      image     = "${aws_ecr_repository.afterlifed.repository_url}:${var.container_image_tag}"
      essential = true
      portMappings = [
        {
          containerPort = 4319
          protocol      = "tcp"
        }
      ]
      environment = [
        { name = "AFTERLIFE_BIND_ADDR", value = "0.0.0.0:4319" },
        { name = "AFTERLIFE_PUBLIC_BASE_URL", value = var.public_base_url },
        { name = "AFTERLIFE_GOOGLE_CLIENT_ID", value = var.google_oidc_client_id },
        { name = "AFTERLIFE_GOOGLE_HOSTED_DOMAIN", value = var.google_workspace_hosted_domain },
        { name = "AFTERLIFE_ARTIFACTS_BUCKET", value = aws_s3_bucket.artifacts.id },
        { name = "AFTERLIFE_TELEMETRY_BUCKET", value = aws_s3_bucket.telemetry.id },
        { name = "AWS_REGION", value = var.aws_region }
      ]
      secrets = [
        {
          name      = "AFTERLIFE_GOOGLE_CLIENT_SECRET"
          valueFrom = aws_secretsmanager_secret.google_oidc_client_secret.arn
        },
        {
          name      = "AFTERLIFE_JWT_SECRET"
          valueFrom = aws_secretsmanager_secret.jwt_signing_key.arn
        },
        {
          name      = "AFTERLIFE_DATABASE_URL"
          valueFrom = aws_secretsmanager_secret.database_url.arn
        }
      ]
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.afterlifed.name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "afterlifed"
        }
      }
    }
  ])
}

resource "aws_ecs_service" "afterlifed" {
  name                               = "${local.name_prefix}-service"
  cluster                            = aws_ecs_cluster.main.id
  task_definition                    = aws_ecs_task_definition.afterlifed.arn
  desired_count                      = var.ecs_desired_count
  launch_type                        = "FARGATE"
  deployment_minimum_healthy_percent = 100
  deployment_maximum_percent         = 200

  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.ecs.id]
    assign_public_ip = true
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.afterlifed.arn
    container_name   = "afterlifed"
    container_port   = 4319
  }

  depends_on = [aws_lb_listener.https]
}
