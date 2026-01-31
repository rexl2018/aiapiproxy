# macOS Service Deployment Guide

This guide explains how to deploy the AI API Proxy as a launchd service on macOS.

## Quick Install (Recommended)

The easiest way to install aiapiproxy as a macOS service:

```bash
# Install and start the service (builds automatically)
./scripts/install-macos-service.sh install
```

That's it! The service will:
- Build the release binary
- Create a symlink at `~/.local/bin/aiapiproxy`
- Install as a user-level LaunchAgent (auto-starts on login)
- Start immediately

### Configuration

Edit your config file before or after installation:

```bash
# Config location
~/.config/aiapiproxy/aiapiproxy.json
```

If no config exists, the example config will be copied automatically.

### Service Management

```bash
# Check status
./scripts/install-macos-service.sh status

# View logs
./scripts/install-macos-service.sh logs

# Rebuild and restart (after code changes)
./scripts/install-macos-service.sh reload

# Uninstall
./scripts/install-macos-service.sh uninstall
```

Or use launchctl directly:

```bash
launchctl start com.aiapiproxy     # Start
launchctl stop com.aiapiproxy      # Stop
launchctl list com.aiapiproxy      # Status
```

### File Locations

| Item | Path |
|------|------|
| Binary (symlink) | `~/.local/bin/aiapiproxy` |
| Config | `~/.config/aiapiproxy/aiapiproxy.json` |
| Logs | `~/.local/var/log/aiapiproxy/` |
| Plist | `~/Library/LaunchAgents/com.aiapiproxy.plist` |

### Health Check

```bash
curl http://127.0.0.1:8082/health
```

---

## Manual Installation (Advanced)

For system-level deployment with dedicated service user, follow these steps.

### Prerequisites

- macOS 10.10+ (Yosemite or later)
- Rust toolchain installed
- Administrator access (sudo)

### Step 1: Build the Application

```bash
cargo build --release
```

### Step 2: Create Service User (Optional)

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

### Step 3: Install Application Files

```bash
# Create directories
sudo mkdir -p /usr/local/bin
sudo mkdir -p /usr/local/etc/aiapiproxy
sudo mkdir -p /usr/local/var/log/aiapiproxy

# Copy binary
sudo cp target/release/aiapiproxy /usr/local/bin/

# Copy configuration
sudo cp aiapiproxy.example.json /usr/local/etc/aiapiproxy/aiapiproxy.json

# Set ownership and permissions
sudo chown _aiapiproxy:_aiapiproxy /usr/local/bin/aiapiproxy
sudo chown -R _aiapiproxy:_aiapiproxy /usr/local/etc/aiapiproxy
sudo chown -R _aiapiproxy:_aiapiproxy /usr/local/var/log/aiapiproxy
sudo chmod +x /usr/local/bin/aiapiproxy
```

### Step 4: Configure

Edit the configuration:

```bash
sudo nano /usr/local/etc/aiapiproxy/aiapiproxy.json
```

### Step 5: Install Launch Daemon

```bash
# Copy the plist file
sudo cp com.aiapiproxy.plist /Library/LaunchDaemons/

# Set proper ownership and permissions
sudo chown root:wheel /Library/LaunchDaemons/com.aiapiproxy.plist
sudo chmod 644 /Library/LaunchDaemons/com.aiapiproxy.plist
```

### Step 6: Load and Start the Service

```bash
# Load the service
sudo launchctl load /Library/LaunchDaemons/com.aiapiproxy.plist

# Start the service
sudo launchctl start com.aiapiproxy

# Check if it's running
sudo launchctl list | grep aiapiproxy
```

---

## Service Management Commands

### User-Level Service (LaunchAgent)

```bash
launchctl start com.aiapiproxy
launchctl stop com.aiapiproxy
launchctl list com.aiapiproxy
launchctl unload ~/Library/LaunchAgents/com.aiapiproxy.plist
launchctl load ~/Library/LaunchAgents/com.aiapiproxy.plist
```

### System-Level Service (LaunchDaemon)

```bash
sudo launchctl start com.aiapiproxy
sudo launchctl stop com.aiapiproxy
sudo launchctl list com.aiapiproxy
sudo launchctl unload /Library/LaunchDaemons/com.aiapiproxy.plist
sudo launchctl load /Library/LaunchDaemons/com.aiapiproxy.plist
```

---

## Monitoring and Logs

### User-Level Logs

```bash
tail -f ~/.local/var/log/aiapiproxy/stderr.log
```

### System-Level Logs

```bash
tail -f /usr/local/var/log/aiapiproxy/stderr.log
```

### System Log

```bash
log show --predicate 'process == "aiapiproxy"' --info
log stream --predicate 'process == "aiapiproxy"'
```

---

## Troubleshooting

### Service Won't Start

1. Check service status:
   ```bash
   launchctl list com.aiapiproxy
   ```

2. Check logs:
   ```bash
   tail -f ~/.local/var/log/aiapiproxy/stderr.log
   ```

3. Verify plist syntax:
   ```bash
   plutil -lint ~/Library/LaunchAgents/com.aiapiproxy.plist
   ```

4. Test binary manually:
   ```bash
   ~/.local/bin/aiapiproxy
   ```

### Port Already in Use

```bash
# Find what's using the port
lsof -i :8082

# Kill the process if needed
kill -9 <PID>
```

### Config Not Found

Ensure config exists at the expected location:
```bash
ls -la ~/.config/aiapiproxy/aiapiproxy.json
```

---

## Uninstallation

### User-Level (Script Install)

```bash
./scripts/install-macos-service.sh uninstall

# Optionally remove config and logs
rm -rf ~/.config/aiapiproxy
rm -rf ~/.local/var/log/aiapiproxy
```

### System-Level (Manual Install)

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

---

## Comparison: User vs System Service

| Feature | User (LaunchAgent) | System (LaunchDaemon) |
|---------|-------------------|----------------------|
| **Sudo Required** | No | Yes |
| **Starts On** | User login | System boot |
| **Plist Location** | `~/Library/LaunchAgents/` | `/Library/LaunchDaemons/` |
| **Runs As** | Current user | Dedicated user |
| **Use Case** | Personal dev machine | Production server |
