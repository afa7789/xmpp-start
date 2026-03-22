-- Test-only Prosody config.
-- TLS and encryption disabled so tests connect over plain TCP to 127.0.0.1.

c2s_require_encryption = false
s2s_require_encryption = false
authentication = "internal_plain"
storage = "internal"
allow_registration = false  -- accounts created by entrypoint.sh

log = { info = "*console"; warn = "*console"; error = "*console" }

VirtualHost "localhost"

modules_enabled = {
    "roster";
    "saslauth";
    "disco";
    "carbons";
    "smacks";
    "mam";
    "ping";
    "posix";
}

archive_expires_after = "1d"
default_archive_policy = true
