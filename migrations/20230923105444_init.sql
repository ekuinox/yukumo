CREATE TABLE IF NOT EXISTS files (
    file_name TEXT NOT NULL,
    file_url TEXT NOT NULL,
    space_id TEXT NOT NULL,
    block_id TEXT NOT NULL,
    origin_file_path TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL default CURRENT_TIMESTAMP,
    PRIMARY KEY (file_name)
)
