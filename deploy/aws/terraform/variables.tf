variable "aws_region" {
  type        = string
  description = "AWS region for all resources."
  default     = "us-east-1"
}

variable "project_name" {
  type        = string
  description = "Short name used in resource names (lowercase, hyphens)."
  default     = "stellarroute"
}

variable "environment" {
  type        = string
  description = "Environment label (staging | production)."
  default     = "staging"
}

variable "vpc_cidr" {
  type        = string
  description = "VPC CIDR block."
  default     = "10.40.0.0/16"
}

variable "availability_zones" {
  type        = list(string)
  description = "AZs for subnets (use two for ALB/RDS)."
  default     = ["us-east-1a", "us-east-1b"]
}

variable "certificate_arn" {
  type        = string
  description = "ACM certificate ARN in the same region as the ALB (HTTPS). Leave empty to create HTTP-only ALB (not recommended for public staging)."
  default     = ""
}

variable "api_image_tag" {
  type        = string
  description = "ECR tag for stellarroute-api."
  default     = "latest"
}

variable "indexer_image_tag" {
  type        = string
  description = "ECR tag for stellarroute-indexer."
  default     = "latest"
}

variable "api_cpu" {
  type    = number
  default = 512
}

variable "api_memory" {
  type    = number
  default = 1024
}

variable "api_desired_count" {
  type    = number
  default = 1
}

variable "indexer_cpu" {
  type    = number
  default = 512
}

variable "indexer_memory" {
  type    = number
  default = 1024
}

variable "indexer_desired_count" {
  type        = number
  description = "Keep at 1 unless you have a multi-writer design."
  default     = 1
}

variable "db_instance_class" {
  type    = string
  default = "db.t4g.micro"
}

variable "db_allocated_storage" {
  type    = number
  default = 20
}

variable "db_name" {
  type    = string
  default = "stellarroute"
}

variable "db_username" {
  type    = string
  default = "stellarroute"
}

variable "rds_deletion_protection" {
  type        = bool
  description = "Protect RDS from terraform destroy. Disable only for disposable staging."
  default     = true
}

variable "redis_node_type" {
  type    = string
  default = "cache.t4g.micro"
}

variable "enable_nat_gateway" {
  type        = bool
  description = "Required for Fargate tasks in private subnets to reach Horizon/Soroban/ECR."
  default     = true
}

variable "single_nat_gateway" {
  type        = bool
  description = "One NAT for all private subnets (cheaper staging)."
  default     = true
}
