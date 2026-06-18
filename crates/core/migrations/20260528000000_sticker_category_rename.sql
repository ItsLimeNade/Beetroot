-- Bonbon 0.3.0 renamed sticker categories. Migrate existing rows.
UPDATE stickers SET category = 'fast_rise'  WHERE category = 'rising';
UPDATE stickers SET category = 'fast_drop'  WHERE category = 'dropping';
UPDATE stickers SET category = 'background' WHERE category = 'other';
