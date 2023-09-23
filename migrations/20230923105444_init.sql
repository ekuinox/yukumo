CREATE TABLE IF NOT EXISTS files (
    file_url TEXT NOT NULL,
    space_id TEXT NOT NULL,
    block_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    PRIMARY KEY (file_url, space_id, block_id)
)
