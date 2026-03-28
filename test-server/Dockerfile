FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    prosody \
    prosody-modules \
    lua-dbi-sqlite3 \
    openssl \
    netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# Prosody data directory
RUN mkdir -p /var/lib/prosody && chown prosody:prosody /var/lib/prosody

EXPOSE 5222 5269 5280 5281

CMD ["prosody", "-F"]
