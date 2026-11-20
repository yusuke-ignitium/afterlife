resource "aws_db_subnet_group" "main" {
  name       = "${local.name_prefix}-db-subnets"
  subnet_ids = aws_subnet.db_private[*].id
}

resource "aws_db_parameter_group" "pg16" {
  name   = "${local.name_prefix}-pg16"
  family = "postgres16"

  # Enforce TLS in transit for all application connections
  parameter {
    name  = "rds.force_ssl"
    value = "1"
  }
}

resource "aws_db_instance" "main" {
  identifier              = "${local.name_prefix}-pg"
  engine                  = "postgres"
  engine_version          = "16.6"
  instance_class          = var.db_instance_class
  allocated_storage       = var.db_allocated_storage_gb
  max_allocated_storage   = var.db_max_allocated_storage_gb
  storage_type            = "gp3"
  storage_encrypted       = true
  db_name                 = "afterlife"
  username                = var.db_master_username
  password                = var.db_master_password
  multi_az                = var.db_multi_az
  publicly_accessible     = false
  db_subnet_group_name    = aws_db_subnet_group.main.name
  vpc_security_group_ids  = [aws_security_group.rds.id]
  parameter_group_name    = aws_db_parameter_group.pg16.name
  backup_retention_period = 7
  deletion_protection     = var.environment == "prod"
  skip_final_snapshot     = var.environment != "prod"
  final_snapshot_identifier = var.environment == "prod" ? "${local.name_prefix}-pg-final" : null
  apply_immediately       = var.environment != "prod"
}
