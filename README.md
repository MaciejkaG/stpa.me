# Short Link Server

A high-performance Rust web server for hosting short links with PostgreSQL backend.

## Features

- **High Performance**: Built with Axum and Tokio for excellent async performance
- **PostgreSQL Backend**: Reliable database storage with connection pooling
- **Click Tracking**: Automatic click counting for analytics
- **Environment Configuration**: Fully configurable via environment variables
- **Health Checks**: Built-in health endpoint for monitoring
- **Logging**: Structured logging with tracing

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgresql://localhost/shortlinks` |
| `BIND_ADDRESS` | Server bind address | `0.0.0.0:3000` |
| `DEFAULT_REDIRECT_URL` | Default redirect for root path | `https://example.com` |
| `RUST_LOG` | Log level | `info` |

## API Endpoints

- `GET /` - Redirects to default URL
- `GET /:token` - Redirects to the long URL for the given token
- `GET /health` - Health check endpoint

## Performance Features

- Connection pooling with SQLx
- Async click count updates (fire-and-forget)
- Efficient database indexes
- Minimal memory allocations
- HTTP/2 support via Axum

## Example Usage

```bash
# Access a short link
curl -L http://localhost:3000/github
# Redirects to https://github.com

# Health check
curl http://localhost:3000/health
# Returns: OK
```