-- Keep every sequence, transfer link and byte; sequence limits now belong to callers.
CREATE TEMP TABLE store_v8_sequence(value INTEGER);
INSERT INTO store_v8_sequence SELECT seq FROM sqlite_sequence WHERE name='frames';
CREATE TABLE frames_v8(sequence INTEGER PRIMARY KEY AUTOINCREMENT, transfer TEXT NOT NULL REFERENCES transfers(id), number INTEGER NOT NULL CHECK(number BETWEEN 0 AND 255), raw BLOB NOT NULL);
INSERT INTO frames_v8 SELECT sequence,transfer,number,raw FROM frames;
DROP TABLE frames;
ALTER TABLE frames_v8 RENAME TO frames;
UPDATE sqlite_sequence SET seq=max(seq,coalesce((SELECT max(value) FROM store_v8_sequence),seq)) WHERE name='frames';
INSERT INTO sqlite_sequence(name,seq) SELECT 'frames',max(value) FROM store_v8_sequence
 HAVING max(value) IS NOT NULL AND NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='frames');
DROP TABLE store_v8_sequence;
CREATE TRIGGER frame_activity AFTER INSERT ON frames WHEN EXISTS(SELECT 1 FROM frame_provenance WHERE transfer=NEW.transfer) BEGIN
 INSERT INTO machine_activity SELECT json_extract(evidence,'$.machine'),unixepoch() FROM frame_provenance WHERE transfer=NEW.transfer
 ON CONFLICT(machine) DO UPDATE SET time=max(time,excluded.time);
END;
CREATE TRIGGER immutable_frames_update BEFORE UPDATE ON frames BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_frames_delete BEFORE DELETE ON frames BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
PRAGMA user_version=8;
