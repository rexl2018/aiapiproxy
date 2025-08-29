# macOS Service Deployment Guide

This guide explains how to deploy the AI API Proxy as a launchd service on macOS.

## Prerequisites

- macOS 10.10+ (Yosemite or later)
- Rust toolchain installed
- Administrator access (sudo)
- Homebrew (recommended for dependencies)

## Step 1: Build the Application

```bash
# Build the release version
cargo build --release

# Verify the binary
./target/release/aiapiproxy --help
```

## Step 2: Create Service User (Optional but Recommended)

```bash
# Create a dedicated user for the service
sudo dscl . -create /Users/_aiapiproxy
sudo dscl . -create /Users/_aiapiproxy UserShell /usr/bin/false
sudo dscl . -create /Users/_aiapiproxy RealName "AI API Proxy Service"
sudo dscl . -create /Users/_aiapiproxy UniqueID 501
sudo dscl . -create /Users/_aiapiproxy PrimaryGroupID 20
sudo dscl . -create /Users/_aiapiproxy NFSHomeDirectory /var/empty
sudo dscl . -passwd /Users/_aiapiproxy '*'

# Create group
sudo dscl . -create /Groups/_aiapiproxy
sudo dscl . -create /Groups/_aiapiproxy RealName "AI API Proxy Service Group"
sudo dscl . -create /Groups/_aiapiproxy PrimaryGroupID 501
sudo dscl . -create /Groups/_aiapiproxy GroupMembership _aiapiproxy
```

## Step 3: Install Application Files

```bash
# Create application directories
sudo mkdir -p /usr/local/bin
sudo mkdir -p /usr/local/etc/aiapiproxy
sudo mkdir -p /usr/local/var/log/aiapiproxy

# Copy binary
sudo cp target/release/aiapiproxy /usr/local/bin/

# Copy configuration
sudo cp .env.example /usr/local/etc/aiapiproxy/.env

# Set ownership and permissions
sudo chown _aiapiproxy:_aiapiproxy /usr/local/bin/aiapiproxy
sudo chown -R _aiapiproxy:_aiapiproxy /usr/local/etc/aiapiproxy
sudo chown -R _aiapiproxy:_aiapiproxy /usr/local/var/log/aiapiproxy

# Set executable permissions
sudo chmod +x /usr/local/bin/aiapiproxy
sudo chmod 600 /usr/local/etc/aiapiproxy/.env
```

## Step 4: Configure Environment

Edit the environment file:

```bash
sudo nano /usr/local/etc/aiapiproxy/.env
```

Update the configuration:

```bash
# Server configuration
SERVER_HOST=127.0.0.1
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

## Step 5: Install Launch Daemon

```bash
# Copy the plist file
sudo cp com.aiapiproxy.plist /Library/LaunchDaemons/

# Set proper ownership and permissions
sudo chown root:wheel /Library/LaunchDaemons/com.aiapiproxy.plist
sudo chmod 644 /Library/LaunchDaemons/com.aiapiproxy.plist
```

## Step 6: Load and Start the Service

```bash
# Load the service
sudo launchctl load /Library/LaunchDaemons/com.aiapiproxy.plist

# Start the service
sudo launchctl start com.aiapiproxy

# Check if it's running
sudo launchctl list | grep aiapiproxy
```

## Service Management Commands

### Basic Operations

```bash
# Start service
sudo launchctl start com.aiapiproxy

# Stop service
sudo launchctl stop com.aiapiproxy

# Restart service (stop then start)
sudo launchctl stop com.aiapiproxy
sudo launchctl start com.aiapiproxy

# Load service (enable auto-start)
sudo launchctl load /Library/LaunchDaemons/com.aiapiproxy.plist

# Unload service (disable auto-start)
sudo launchctl unload /Library/LaunchDaemons/com.aiapiproxy.plist

# Check service status
sudo launchctl list com.aiapiproxy

# View all loaded services
sudo launchctl list | grep aiapiproxy
```

### Advanced Management

```bash
# Reload service configuration
sudo launchctl unload /Library/LaunchDaemons/com.aiapiproxy.plist
sudo launchctl load /Library/LaunchDaemons/com.aiapiproxy.plist

# Enable service (load and start)
sudo launchctl enable system/com.aiapiproxy

# Disable service
sudo launchctl disable system/com.aiapiproxy

# Bootstrap service (macOS 11+)
sudo launchctl bootstrap system /Library/LaunchDaemons/com.aiapiproxy.plist

# Bootout service (macOS 11+)
sudo launchctl bootout system /Library/LaunchDaemons/com.aiapiproxy.plist
```

## Monitoring and Logs

### Log Files

```bash
# View stdout logs
tail -f /usr/local/var/log/aiapiproxy/stdout.log

# View stderr logs
tail -f /usr/local/var/log/aiapiproxy/stderr.log

# View system logs
log show --predicate 'process == "aiapiproxy"' --info

# Stream live logs
log stream --predicate 'process == "aiapiproxy"'
```

### Console.app

1. Open **Console.app**
2. Search for "aiapiproxy" in the search bar
3. Filter by process name or subsystem

## Health Check

Verify the service is running correctly:

```bash
# Check if service is listening
lsof -i :8082

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

### Built-in Firewall

```bash
# Allow incoming connections (if needed)
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /usr/local/bin/aiapiproxy
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp /usr/local/bin/aiapiproxy
```

### pfctl (Advanced)

Create `/etc/pf.anchors/aiapiproxy`:

```
# Allow API proxy traffic
pass in on lo0 proto tcp from any to any port 8082
pass out on en0 proto tcp from any to any port 443
pass out on en0 proto tcp from any to any port 80
```

Load the rules:

```bash
sudo pfctl -f /etc/pf.conf
```

## Reverse Proxy Setup (Optional)

### Nginx via Homebrew

```bash
# Install nginx
brew install nginx

# Create configuration
sudo mkdir -p /usr/local/etc/nginx/sites-available
sudo mkdir -p /usr/local/etc/nginx/sites-enabled
```

Create `/usr/local/etc/nginx/sites-available/aiapiproxy`:

```nginx
server {
    listen 80;
    server_name localhost;
    
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
ln -s /usr/local/etc/nginx/sites-available/aiapiproxy /usr/local/etc/nginx/sites-enabled/
nginx -t
brew services restart nginx
```

## Updating the Service

```bash
# Stop the service
sudo launchctl stop com.aiapiproxy

# Build new version
cargo build --release

# Update binary
sudo cp target/release/aiapiproxy /usr/local/bin/
sudo chown _aiapiproxy:_aiapiproxy /usr/local/bin/aiapiproxy
sudo chmod +x /usr/local/bin/aiapiproxy

# Start the service
sudo launchctl start com.aiapiproxy

# Verify update
sudo launchctl list com.aiapiproxy
```

## Troubleshooting

### Service Won't Start

1. Check service status:
   ```bash
   sudo launchctl list com.aiapiproxy
   ```

2. Check logs:
   ```bash
   tail -f /usr/local/var/log/aiapiproxy/stderr.log
   ```

3. Verify plist syntax:
   ```bash
   plutil -lint /Library/LaunchDaemons/com.aiapiproxy.plist
   ```

4. Test binary manually:
   ```bash
   sudo -u _aiapiproxy /usr/local/bin/aiapiproxy
   ```

### Permission Issues

```bash
# Fix ownership
sudo chown _aiapiproxy:_aiapiproxy /usr/local/bin/aiapiproxy
sudo chown -R _aiapiproxy:_aiapiproxy /usr/local/etc/aiapiproxy
sudo chown -R _aiapiproxy:_aiapiproxy /usr/local/var/log/aiapiproxy

# Fix permissions
sudo chmod +x /usr/local/bin/aiapiproxy
sudo chmod 600 /usr/local/etc/aiapiproxy/.env
```

### Port Already in Use

```bash
# Find what's using the port
lsof -i :8082

# Kill the process if needed
sudo kill -9 <PID>
```

### Service Keeps Crashing

1. Check crash logs:
   ```bash
   ls -la ~/Library/Logs/DiagnosticReports/ | grep aiapiproxy
   ```

2. Increase throttle interval in plist:
   ```xml
   <key>ThrottleInterval</key>
   <integer>30</integer>
   ```

3. Check system resources:
   ```bash
   top -pid $(pgrep aiapiproxy)
   ```

## Security Considerations

1. **API Keys**: Store sensitive API keys securely in the `.env` file with restricted permissions (600)
2. **User Isolation**: Run the service under a dedicated user account (`_aiapiproxy`)
3. **Network Security**: Bind to localhost (127.0.0.1) by default
4. **File Permissions**: Restrict access to configuration and log files
5. **Firewall**: Configure application firewall rules if needed

## Performance Tuning

### Resource Limits

Adjust limits in the plist file:

```xml
<key>HardResourceLimits</key>
<dict>
    <key>NumberOfFiles</key>
    <integer>100000</integer>
    <key>NumberOfProcesses</key>
    <integer>8192</integer>
    <key>ResidentSetSize</key>
    <integer>1073741824</integer> <!-- 1GB -->
</dict>
```

### Environment Variables

Add performance-related environment variables:

```xml
<key>EnvironmentVariables</key>
<dict>
    <key>RUST_LOG</key>
    <string>info</string>
    <key>RUST_BACKTRACE</key>
    <string>1</string>
    <key>TOKIO_WORKER_THREADS</key>
    <string>4</string>
</dict>
```

## Log Rotation

Create `/etc/newsyslog.d/aiapiproxy.conf`:

```
# logfilename                                    [owner:group]    mode count size when  flags [/pid_file] [sig_num]
/usr/local/var/log/aiapiproxy/stdout.log        _aiapiproxy:_aiapiproxy  644  7     1000  *     J
/usr/local/var/log/aiapiproxy/stderr.log        _aiapiproxy:_aiapiproxy  644  7     1000  *     J
```

## Uninstallation

```bash
# Stop and unload service
sudo launchctl stop com.aiapiproxy
sudo launchctl unload /Library/LaunchDaemons/com.aiapiproxy.plist

# Remove files
sudo rm /Library/LaunchDaemons/com.aiapiproxy.plist
sudo rm /usr/local/bin/aiapiproxy
sudo rm -rf /usr/local/etc/aiapiproxy
sudo rm -rf /usr/local/var/log/aiapiproxy

# Remove user and group (optional)
sudo dscl . -delete /Users/_aiapiproxy
sudo dscl . -delete /Groups/_aiapiproxy
```

## Comparison: macOS vs Linux Service Management

| Feature | macOS (launchd) | Linux (systemd) |
|---------|-----------------|------------------|
| **Service File** | `.plist` (XML) | `.service` (INI) |
| **Location** | `/Library/LaunchDaemons/` | `/etc/systemd/system/` |
| **Start Command** | `launchctl start` | `systemctl start` |
| **Auto-start** | `RunAtLoad` | `enable` |
| **Logs** | File-based + Console.app | journalctl |
| **Resource Limits** | Built-in plist keys | systemd directives |

This deployment guide provides a production-ready setup for running your AI API Proxy as a macOS service with proper process management, logging, and security configurations.