FROM prosody/prosody:latest

USER root

# Copy test config and startup script.
COPY prosody.cfg.lua /etc/prosody/conf.avail/localhost.cfg.lua
COPY entrypoint.sh   /usr/local/bin/test-entrypoint.sh
RUN chmod +x /usr/local/bin/test-entrypoint.sh \
 && ln -sf /etc/prosody/conf.avail/localhost.cfg.lua \
           /etc/prosody/conf.d/localhost.cfg.lua

USER prosody

EXPOSE 5222

ENTRYPOINT ["/usr/local/bin/test-entrypoint.sh"]
