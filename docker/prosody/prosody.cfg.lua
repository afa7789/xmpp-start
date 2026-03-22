-- Minimal Prosody config for local development / integration testing.
-- Not for production use.

-- Disable TLS requirement so the app can connect with plain TCP on localhost.
c2s_require_encryption = false
s2s_require_encryption = false

-- Allow plain-text password auth (fine for local testing).
authentication = "internal_plain"

-- Storage backend.
storage = "internal"

-- Log to stdout so docker logs show everything.
log = {
    info = "*console";
    warn = "*console";
    error = "*console";
}

-- The test virtual host.
VirtualHost "localhost"

-- MUC component for group chat testing.
Component "conference.localhost" "muc"
    modules_enabled = { "muc_mam" }

-- Core modules needed for the app.
modules_enabled = {
    "roster";        -- XEP-0237 roster versioning
    "saslauth";      -- authentication
    "dialback";      -- s2s auth
    "disco";         -- XEP-0030 service discovery
    "carbons";       -- XEP-0280 message carbons
    "smacks";        -- XEP-0198 stream management
    "mam";           -- XEP-0313 message archive
    "ping";          -- XEP-0199 keep-alive
    "pep";           -- XEP-0163 personal eventing (avatars)
    "vcard4";        -- vCard avatars
    "register";      -- in-band registration (disabled below)
    "posix";         -- POSIX compatibility
}

-- Disable public registration (we create accounts via prosodyctl).
allow_registration = false

-- MAM: store all messages for 30 days.
archive_expires_after = "30d"
default_archive_policy = true
