# Box Fraise Platform — Deployment Runbook

Hardening §8. Procedures for bringing up a fresh VPS and pushing a build to it.

---

## Prerequisites

- Ubuntu 24.04 LTS VPS (DigitalOcean Droplet, 2 GB RAM minimum).
- DNS A record: `api.boxfraise.com` → VPS IP.
- SSH access as `root` (or `sudo`-able user).

---

## One-time server setup

### 1. Create the application user

```sh
adduser --system --shell /bin/bash --home /opt/box-fraise-platform --group boxfraise
mkdir -p /opt/box-fraise-platform/migrations
chown -R boxfraise:boxfraise /opt/box-fraise-platform
```

### 2. Install OS packages

```sh
apt update
apt install -y nginx certbot python3-certbot-nginx \
               postgresql-client redis-tools ufw
```

### 3. Firewall

```sh
ufw allow 22/tcp     # SSH
ufw allow 80/tcp     # nginx
ufw allow 443/tcp    # nginx-tls
ufw --force enable
```

### 4. SSL certificate

```sh
certbot --nginx -d api.boxfraise.com --non-interactive --agree-tos -m ops@boxfraise.com
```

`certbot` will install its own renewal cron / timer.

### 5. Install nginx config

```sh
cp deploy/nginx.conf /etc/nginx/sites-available/box-fraise-platform
ln -sf /etc/nginx/sites-available/box-fraise-platform /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx
```

### 6. Install systemd unit

```sh
cp deploy/box-fraise-platform.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable box-fraise-platform
```

(Don't `start` yet — there's no binary or `.env` on disk.)

---

## Application deployment

### 1. Stage the binary and migrations

From a build host (or a release pipeline):

```sh
scp target/release/server          root@vps:/opt/box-fraise-platform/server
scp -r server/migrations/          root@vps:/opt/box-fraise-platform/migrations
chown -R boxfraise:boxfraise       /opt/box-fraise-platform
chmod +x /opt/box-fraise-platform/server
```

### 2. Configure environment

`/opt/box-fraise-platform/.env` — populate every required key from
`server/.env.example`. Required (server fails fast on any missing):

- `DATABASE_URL`
- `JWT_SECRET`, `STAFF_JWT_SECRET`
- `ADMIN_PIN`, `CHOCOLATIER_PIN`, `SUPPLIER_PIN`
- `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`
- `SOULTOKEN_HMAC_KEY`
- `SOULTOKEN_SIGNING_KEY_HEX` (mint with `scripts/generate_ed25519_key.sh`)
- `SOULTOKEN_VERIFYING_KEY_HEX` (must match the public key derived from the signing key — server logs it on first boot)

Optional but recommended in production:

- `ALLOWED_ORIGINS` — set to the actual web frontend origin(s).
- `APP_USER_DATABASE_URL` — switch to non-superuser role to enforce RLS.
- `SPACES_*` — enable evidence storage.
- `SENTRY_DSN` — error tracking.
- `REDIS_URL` — required before scaling beyond one instance (nonce dedup).

```sh
chown boxfraise:boxfraise /opt/box-fraise-platform/.env
chmod 600                  /opt/box-fraise-platform/.env
```

### 3. Run migrations

```sh
sudo -u boxfraise -- bash -c '
  cd /opt/box-fraise-platform &&
  source .env &&
  /usr/local/bin/sqlx migrate run --source migrations
'
```

### 4. Start

```sh
systemctl start box-fraise-platform
systemctl status box-fraise-platform   # confirm it bound and didn't exit
journalctl -u box-fraise-platform -f   # tail logs
```

### 5. Smoke check

```sh
curl -sS https://api.boxfraise.com/health | jq .
# expect: {"status":"healthy","database":"ok",...}
```

---

## Rollback procedure

If a deploy goes bad:

```sh
systemctl stop box-fraise-platform
mv /opt/box-fraise-platform/server.previous /opt/box-fraise-platform/server
systemctl start box-fraise-platform
curl -sS https://api.boxfraise.com/health | jq .
```

Always keep the previous binary around until the new one has run a full
business cycle (24 h minimum).

---

## What this runbook doesn't cover yet

- Automated deploy pipeline (see commented `staging-deploy` job in `.github/workflows/ci.yml`).
- Database backup / restore — wire up `pg_dump` to S3 daily before any production data lands.
- Log shipping — journald only, not yet aggregated to a central store.
- Hot-rotation of `SOULTOKEN_HMAC_KEY` (multi-version key store is TODO — see `derive_display_code` doc).
