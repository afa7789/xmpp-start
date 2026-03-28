#!/usr/bin/env bash
# test-server/setup.sh
# Creates test accounts and pre-configures the MUC room.
# Runs inside the Prosody container after the server starts.

set -euo pipefail

echo "==> Creating test accounts..."

prosodyctl register alice localhost alice123 2>/dev/null || echo "  alice already exists"
prosodyctl register bob localhost bob123 2>/dev/null || echo "  bob already exists"
prosodyctl register charlie localhost charlie123 2>/dev/null || echo "  charlie already exists"
prosodyctl register admin localhost admin123 2>/dev/null || echo "  admin already exists"

echo "==> Verifying accounts..."
for user in alice bob charlie admin; do
    if prosodyctl user exists "${user}@localhost" 2>/dev/null; then
        echo "  OK: ${user}@localhost"
    else
        echo "  WARN: ${user}@localhost might not exist (check manually)"
    fi
done

echo "==> Pre-creating testroom@conference.localhost..."

prosodyctl shell <<'LUAEOF' 2>/dev/null || echo "  Room will be created on first join instead."
local room_jid = "testroom@conference.localhost"
local muc_host = prosody.hosts["conference.localhost"]
if muc_host then
    local muc = muc_host.modules.muc
    if muc then
        local room = muc.get_room(room_jid)
        if not room then
            room = muc.create_room(room_jid)
            if room then
                room:set_name("Test Room")
                room:set_description("Pre-configured test room for ReXisCe development")
                room:set_persistent(true)
                room:set_public(true)
                room:set_members_only(false)
                room:set_moderated(false)
                room:set_whois("anyone")
                room:save()
                print("Room created: " .. room_jid)
            end
        else
            print("Room already exists: " .. room_jid)
        end
    else
        print("MUC module not loaded on conference.localhost")
    end
else
    print("conference.localhost host not found")
end
LUAEOF

echo ""
echo "============================================"
echo "  XMPP Test Server Ready"
echo "============================================"
echo "  Domain:     localhost"
echo "  C2S port:   5222"
echo "  HTTP port:  5280"
echo "  HTTPS port: 5281"
echo ""
echo "  Accounts:"
echo "    alice@localhost   / alice123"
echo "    bob@localhost     / bob123"
echo "    charlie@localhost / charlie123"
echo "    admin@localhost   / admin123"
echo ""
echo "  MUC:     testroom@conference.localhost"
echo "  Upload:  upload.localhost"
echo "============================================"
