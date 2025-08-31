-- Short Links Database Schema
-- Run this to create the required table structure

CREATE TABLE IF NOT EXISTS short_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token VARCHAR(255) UNIQUE NOT NULL,
    long_url TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    click_count BIGINT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_short_links_token ON short_links(token);
CREATE INDEX IF NOT EXISTS idx_short_links_active ON short_links(is_active);
CREATE INDEX IF NOT EXISTS idx_short_links_created_at ON short_links(created_at);

-- Example data (optional)
INSERT INTO short_links (token, long_url) VALUES 
    ('github', 'https://github.com'),
    ('rust', 'https://www.rust-lang.org'),
    ('docs', 'https://doc.rust-lang.org')
ON CONFLICT (token) DO NOTHING;