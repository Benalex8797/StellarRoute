#!/usr/bin/env bash
# Build Dockerfile.api / Dockerfile.indexer and push to ECR.
# Prerequisites: aws CLI, docker, terraform apply (ECR repos exist).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TF_DIR="${ROOT}/deploy/aws/terraform"
IMAGE_TAG="${IMAGE_TAG:-$(git -C "${ROOT}" rev-parse --short HEAD)}"
PUSH_LATEST="${PUSH_LATEST:-1}"

AWS_REGION="${AWS_REGION:-}"
if [[ -z "${AWS_REGION}" && -f "${TF_DIR}/terraform.tfvars" ]]; then
  AWS_REGION="$(grep -E '^\s*aws_region\s*=' "${TF_DIR}/terraform.tfvars" | head -1 | sed -E 's/.*=\s*"?([^"]+)"?.*/\1/' || true)"
fi
AWS_REGION="${AWS_REGION:-us-east-1}"

cd "${TF_DIR}"
API_REPO="$(terraform output -raw ecr_api_repository_url)"
INDEXER_REPO="$(terraform output -raw ecr_indexer_repository_url)"
ACCOUNT_ID="$(aws sts get-caller-identity --query Account --output text)"

echo "==> Logging into ECR ${ACCOUNT_ID} (${AWS_REGION})"
aws ecr get-login-password --region "${AWS_REGION}" \
  | docker login --username AWS --password-stdin "${ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com"

echo "==> Building API → ${API_REPO}:${IMAGE_TAG}"
docker build -f "${ROOT}/Dockerfile.api" -t "${API_REPO}:${IMAGE_TAG}" "${ROOT}"
docker push "${API_REPO}:${IMAGE_TAG}"

echo "==> Building indexer → ${INDEXER_REPO}:${IMAGE_TAG}"
docker build -f "${ROOT}/Dockerfile.indexer" -t "${INDEXER_REPO}:${IMAGE_TAG}" "${ROOT}"
docker push "${INDEXER_REPO}:${IMAGE_TAG}"

if [[ "${PUSH_LATEST}" == "1" ]]; then
  docker tag "${API_REPO}:${IMAGE_TAG}" "${API_REPO}:latest"
  docker tag "${INDEXER_REPO}:${IMAGE_TAG}" "${INDEXER_REPO}:latest"
  docker push "${API_REPO}:latest"
  docker push "${INDEXER_REPO}:latest"
fi

echo "==> Done"
echo "    API:     ${API_REPO}:${IMAGE_TAG}"
echo "    Indexer: ${INDEXER_REPO}:${IMAGE_TAG}"
echo
CLUSTER="$(terraform output -raw ecs_cluster_name)"
API_SVC="$(terraform output -raw api_service_name)"
IDX_SVC="$(terraform output -raw indexer_service_name)"
echo "Force ECS rollout:"
echo "  aws ecs update-service --cluster ${CLUSTER} --service ${API_SVC} --force-new-deployment"
echo "  aws ecs update-service --cluster ${CLUSTER} --service ${IDX_SVC} --force-new-deployment"
