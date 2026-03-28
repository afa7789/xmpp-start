-- test-server/prosody.cfg.lua
-- Prosody 0.12 config for local ReXisCe GUI testing.
-- Supports: MUC, MAM, OMEMO relay, HTTP Upload, vCard, Bookmarks, Stream Management.

---------- Server-wide settings ----------

admins = { "admin@localhost" }

-- Network
network_backend = "epoll"
interfaces = { "*" }
c2s_ports = { 5222 }
s2s_ports = { 5269 }
http_ports = { 5280 }
https_ports = {}

-- TLS certificates (generated at container startup)
certificates = "/var/lib/prosody"

-- Authentication — plain text for local testing
authentication = "internal_plain"

-- Local testing: no encryption required
c2s_require_encryption = false
s2s_require_encryption = false
allow_unencrypted_plain_auth = true

-- Storage
default_storage = "internal"
storage = "internal"

-- Logging
log = {
    info = "*console";
    -- Uncomment for debug:
    -- debug = "*console";
}

---------- Global modules ----------

modules_enabled = {
    -- Core RFC 6120/6121
    "roster";
    "saslauth";
    "tls";
    "dialback";
    "disco";
    "private";
    "admin_adhoc";

    -- XEP-0280: Message Carbons
    "carbons";

    -- XEP-0163: Personal Eventing Protocol (PEP) — required for OMEMO, avatars
    "pep";

    -- XEP-0191: Blocking Command
    "blocklist";

    -- XEP-0054: vCard-temp
    "vcard_legacy";

    -- XEP-0092: Software Version
    "version";

    -- XEP-0199: XMPP Ping
    "ping";

    -- XEP-0077: In-Band Registration
    "register";

    -- XEP-0313: Message Archive Management
    "mam";

    -- XEP-0198: Stream Management
    "smacks";

    -- XEP-0048: Bookmarks
    "bookmarks";

    -- XEP-0352: Client State Indication
    "csi_simple";

    -- Server uptime/time
    "uptime";
    "time";

    -- HTTP server
    "http";
}

modules_disabled = {
    "s2s"; -- Not needed for local testing
}

---------- MAM settings ----------

archive_expires_after = "1w"
default_archive_policy = true
max_archive_query_results = 100

---------- Stream Management ----------

smacks_hibernation_time = 600
smacks_max_unacked_stanzas = 0

---------- Registration ----------

allow_registration = true
registration_throttle_max = 0

---------- VirtualHost ----------

VirtualHost "localhost"

---------- Components ----------

-- XEP-0045: Multi-User Chat
Component "conference.localhost" "muc"
    name = "Chatrooms"
    restrict_room_creation = false
    max_history_messages = 100
    modules_enabled = {
        "muc_mam";
    }
    muc_log_by_default = true
    muc_log_presences = false
    muc_log_expires_after = "1w"

-- XEP-0363: HTTP File Upload
Component "upload.localhost" "http_file_share"
    http_file_share_size_limit = 10485760  -- 10 MB
    http_file_share_expires_after = 604800 -- 1 week
    http_host = "localhost"
    http_external_url = "http://localhost:5280"
