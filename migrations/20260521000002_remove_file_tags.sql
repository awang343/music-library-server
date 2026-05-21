-- The scanner no longer imports embedded tags into track_tags.
-- Drop any existing non-user rows and orphaned interned tag values.
DELETE FROM track_tags WHERE source != 'user';
DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM track_tags);
