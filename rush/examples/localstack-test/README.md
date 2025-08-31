# LocalStack Example

This example demonstrates how to use LocalStack with Rush for local AWS service emulation.

## Features

- LocalStack with persistent data storage
- Multiple AWS services (S3, DynamoDB, SQS, SNS, Lambda, Secrets Manager)
- Automatic environment variable injection
- Data persistence between restarts

## Directory Structure

When you run this example, Rush will create:

```
target/local-services/
└── aws_local/
    └── localstack.data/   # Persistent LocalStack data
```

## Running the Example

```bash
# Start LocalStack
rush localstack-test dev

# In another terminal, interact with LocalStack
aws --endpoint-url=http://localhost:4566 s3 mb s3://test-bucket
aws --endpoint-url=http://localhost:4566 s3 ls

# Create a DynamoDB table
aws --endpoint-url=http://localhost:4566 dynamodb create-table \
    --table-name test-table \
    --attribute-definitions AttributeName=id,AttributeType=S \
    --key-schema AttributeName=id,KeyType=HASH \
    --billing-mode PAY_PER_REQUEST

# List tables
aws --endpoint-url=http://localhost:4566 dynamodb list-tables
```

## Data Persistence

With `persist_data: true` and the persistence configuration, LocalStack will:

1. Save state on shutdown to `./target/local-services/aws_local/localstack.data/`
2. Restore state on startup from the same directory
3. Preserve all created resources (S3 buckets, DynamoDB tables, etc.)

## Testing Persistence

```bash
# Create resources
aws --endpoint-url=http://localhost:4566 s3 mb s3://persistent-bucket
aws --endpoint-url=http://localhost:4566 s3 cp README.md s3://persistent-bucket/

# Stop Rush (Ctrl+C)

# Restart Rush
rush localstack-test dev

# Verify resources still exist
aws --endpoint-url=http://localhost:4566 s3 ls s3://persistent-bucket/
```

## Environment Variables

The following environment variables are automatically injected:

- `LOCALSTACK_ENDPOINT`: `http://aws_local:4566`
- `AWS_ENDPOINT_URL`: `http://aws_local:4566`
- `AWS_DEFAULT_REGION`: `us-east-1`
- `AWS_ACCESS_KEY_ID`: `test`
- `AWS_SECRET_ACCESS_KEY`: `test`

## Using from Application Code

### Python Example

```python
import boto3
import os

# S3
s3 = boto3.client(
    's3',
    endpoint_url=os.environ['AWS_ENDPOINT_URL'],
    region_name=os.environ['AWS_DEFAULT_REGION']
)

# DynamoDB
dynamodb = boto3.resource(
    'dynamodb',
    endpoint_url=os.environ['AWS_ENDPOINT_URL'],
    region_name=os.environ['AWS_DEFAULT_REGION']
)

# SQS
sqs = boto3.client(
    'sqs',
    endpoint_url=os.environ['AWS_ENDPOINT_URL'],
    region_name=os.environ['AWS_DEFAULT_REGION']
)
```

### Node.js Example

```javascript
const AWS = require('aws-sdk');

AWS.config.update({
    endpoint: process.env.AWS_ENDPOINT_URL,
    region: process.env.AWS_DEFAULT_REGION
});

const s3 = new AWS.S3();
const dynamodb = new AWS.DynamoDB();
const sqs = new AWS.SQS();
```

## LocalStack Web UI

LocalStack doesn't have a built-in web UI, but you can check service health:

```bash
curl http://localhost:4566/_localstack/health
```

## Troubleshooting

### Check if LocalStack is running:
```bash
docker ps | grep localstack
```

### View LocalStack logs:
```bash
docker logs rush-local-aws_local
```

### Clear all LocalStack data:
```bash
rm -rf ./target/local-services/aws_local/
```

### Verify volume mounts:
```bash
docker inspect rush-local-aws_local | grep -A 10 Mounts
```

You should see:
- `/var/run/docker.sock` mounted for Docker access
- `./target/local-services/aws_local/localstack.data` mounted to `/var/lib/localstack`