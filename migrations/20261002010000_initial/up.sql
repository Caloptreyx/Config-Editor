CREATE TABLE dev_caloptreyx_configeditor_favorites (
    uuid uuid NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_uuid uuid NOT NULL REFERENCES users(uuid) ON DELETE CASCADE,
    server_uuid uuid NOT NULL REFERENCES servers(uuid) ON DELETE CASCADE,
    path VARCHAR(4096) NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_uuid, server_uuid, path)
);
