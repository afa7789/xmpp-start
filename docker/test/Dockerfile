FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    prosody \
    prosody-modules \
    lua-dbi-sqlite3 \
    openssl \
    netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# Copy test config and entrypoint
COPY prosody.cfg.lua /etc/prosody/prosody.cfg.lua
COPY entrypoint.sh   /usr/local/bin/test-entrypoint.sh
RUN chmod +x /usr/local/bin/test-entrypoint.sh \
 && mkdir -p /var/run/prosody \
 && chown prosody:prosody /var/run/prosody /var/lib/prosody \
 && mkdir -p /etc/prosody/certs \
 && openssl req -x509 -newkey rsa:2048 -keyout /etc/prosody/certs/localhost.key \
      -out /etc/prosody/certs/localhost.crt -days 365 -nodes \
      -subj "/CN=localhost" 2>/dev/null \
 && chown -R prosody:prosody /etc/prosody

USER prosody

EXPOSE 5222

ENTRYPOINT ["/usr/local/bin/test-entrypoint.sh"]
