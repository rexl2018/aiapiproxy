# Linux Service Deployment Guide

This guide explains how to deploy the AI API Proxy as a systemd service on Linux.

## Prerequisites

- Linux system with systemd (Ubuntu 16.04+, CentOS 7+, etc.)
- Rust toolchain installed
- Root or sudo access

## Step 1: Build the Application

```bash
# Build the release version
cargo build --release

# Verify the binary
./target/release/aiapiproxy --help
```

## Step 2: Create Service User

```bash
# Create a dedicated user for the service
sudo useradd --system --shell /bin/false --home-dir /opt/aiapiproxy --create-home aiapiproxy
```

## Step 3: Install Application Files

```bash
# Create application directory
sudo mkdir -p /opt/aiapiproxy/{logs,target/release}

# Copy binary
sudo cp target/release/aiapiproxy /opt/aiapiproxy/target/release/

# Copy configuration
sudo cp .env.example /opt/aiapiproxy/.env

# Set ownership
sudo chown -R aiapiproxy:aiapiproxy /opt/aiapiproxy

# Set permissions
sudo chmod +x /opt/aiapiproxy/target/release/aiapiproxy
sudo chmod 600 /opt/aiapiproxy/.env
```

## Step 4: Configure Environment

Edit the environment file:

```bash
sudo nano /opt/aiapiproxy/.env
```

Update the configuration:

```bash
# Server configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=8082

# OpenAI API configuration
OPENAI_API_KEY=your-openai-api-key-here
OPENAI_BASE_URL=https://api.openai.com/v1

# Model mapping
CLAUDE_HAIKU_MODEL=gpt-4o-mini
CLAUDE_SONNET_MODEL=gpt-4o
CLAUDE_OPUS_MODEL=gpt-4

# Timeout configuration
REQUEST_TIMEOUT=30
STREAM_TIMEOUT=300

# Logging
RUST_LOG=info
LOG_FORMAT=json

# Security
ALLOWED_ORIGINS=*
API_KEY_HEADER=Authorization
```

## Step 5: Install Systemd Service

```bash
# Copy service file
sudo cp aiapiproxy.service /etc/systemd/system/

# Reload systemd
sudo systemctl daemon-reload

# Enable service (start on boot)
sudo systemctl enable aiapiproxy
```

## Step 6: Start the Service

```bash
# Start the service
sudo systemctl start aiapiproxy

# Check status
sudo systemctl status aiapiproxy
```

## Service Management Commands

### Basic Operations

```bash
# Start service
sudo systemctl start aiapiproxy

# Stop service
sudo systemctl stop aiapiproxy

# Restart service
sudo systemctl restart aiapiproxy

# Reload configuration (graceful restart)
sudo systemctl reload aiapiproxy

# Check status
sudo systemctl status aiapiproxy

# Enable auto-start on boot
sudo systemctl enable aiapiproxy

# Disable auto-start
sudo systemctl disable aiapiproxy
```

### Monitoring and Logs

```bash
# View logs (real-time)
sudo journalctl -u aiapiproxy -f

# View recent logs
sudo journalctl -u aiapiproxy --since "1 hour ago"

# View logs from today
sudo journalctl -u aiapiproxy --since today

# View logs with specific priority
sudo journalctl -u aiapiproxy -p err
```

## Health Check

Verify the service is running correctly:

```bash
# Check if service is listening
sudo netstat -tlnp | grep :8082

# Test health endpoint
curl http://localhost:8082/health

# Test API endpoint
curl -X POST http://localhost:8082/v1/messages \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 100,
    "messages": [
      {"role": "user", "content": "Hello, world!"}
    ]
  }'
```

## Firewall Configuration

If using a firewall, allow the service port:

```bash
# UFW (Ubuntu)
sudo ufw allow 8082/tcp

# firewalld (CentOS/RHEL)
sudo firewall-cmd --permanent --add-port=8082/tcp
sudo firewall-cmd --reload

# iptables
sudo iptables -A INPUT -p tcp --dport 8082 -j ACCEPT
```

## Reverse Proxy Setup (Optional)

### Nginx Configuration

Create `/etc/nginx/sites-available/aiapiproxy`:

```nginx
server {
    listen 80;
    server_name your-domain.com;
    
    location / {
        proxy_pass http://127.0.0.1:8082;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # For streaming responses
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 300s;
        proxy_connect_timeout 75s;
    }
}
```

Enable the site:

```bash
sudo ln -s /etc/nginx/sites-available/aiapiproxy /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

## Updating the Service

```bash
# Stop the service
sudo systemctl stop aiapiproxy

# Build new version
cargo build --release

# Update binary
sudo cp target/release/aiapiproxy /opt/aiapiproxy/target/release/

# Update ownership
sudo chown aiapiproxy:aiapiproxy /opt/aiapiproxy/target/release/aiapiproxy

# Start the service
sudo systemctl start aiapiproxy

# Verify update
sudo systemctl status aiapiproxy
```

## Troubleshooting

### Service Won't Start

1. Check service status:
   ```bash
   sudo systemctl status aiapiproxy
   ```

2. Check logs:
   ```bash
   sudo journalctl -u aiapiproxy --no-pager
   ```

3. Verify configuration:
   ```bash
   sudo -u aiapiproxy /opt/aiapiproxy/target/release/aiapiproxy --help
   ```

4. Check file permissions:
   ```bash
   ls -la /opt/aiapiproxy/
   ```

### Port Already in Use

```bash
# Find what's using the port
sudo lsof -i :8082

# Kill the process if needed
sudo kill -9 <PID>
```

### Permission Issues

```bash
# Fix ownership
sudo chown -R aiapiproxy:aiapiproxy /opt/aiapiproxy

# Fix permissions
sudo chmod +x /opt/aiapiproxy/target/release/aiapiproxy
sudo chmod 600 /opt/aiapiproxy/.env
```

## Security Considerations

1. **API Keys**: Store sensitive API keys securely in the `.env` file with restricted permissions (600)
2. **User Isolation**: Run the service under a dedicated user account
3. **Network Security**: Use firewall rules to restrict access
4. **HTTPS**: Use a reverse proxy with SSL/TLS for production
5. **Monitoring**: Set up log monitoring and alerting

## Performance Tuning

### System Limits

Edit `/etc/security/limits.conf`:

```
aiapiproxy soft nofile 65536
aiapiproxy hard nofile 65536
aiapiproxy soft nproc 4096
aiapiproxy hard nproc 4096
```

### Service Configuration

Adjust the systemd service file for your needs:

```ini
# Increase file descriptor limits
LimitNOFILE=100000

# Adjust process limits
LimitNPROC=8192

# Set CPU/Memory limits if needed
CPUQuota=200%
MemoryLimit=1G
```

## Monitoring Setup

### Log Rotation

Create `/etc/logrotate.d/aiapiproxy`:

```
/opt/aiapiproxy/logs/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 644 aiapiproxy aiapiproxy
    postrotate
        systemctl reload aiapiproxy
    endscript
}
```

### Health Check Script

Create `/opt/aiapiproxy/health-check.sh`:

```bash
#!/bin/bash
response=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8082/health)
if [ $response -eq 200 ]; then
    echo "Service is healthy"
    exit 0
else
    echo "Service is unhealthy (HTTP $response)"
    exit 1
fi
```

Make it executable:

```bash
sudo chmod +x /opt/aiapiproxy/health-check.sh
```

This deployment guide provides a production-ready setup for running your AI API Proxy as a Linux service with proper security, monitoring, and management capabilities.