CREATE TABLE observation_revisions(identity_key TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0), receipt TEXT NOT NULL, interpretation_version INTEGER NOT NULL, content_digest BLOB NOT NULL CHECK(length(content_digest)=32), PRIMARY KEY(identity_key,revision), FOREIGN KEY(receipt,interpretation_version) REFERENCES interpretations(receipt,version));
CREATE TRIGGER observation_revisions_immutable_update BEFORE UPDATE ON observation_revisions BEGIN SELECT RAISE(ABORT,'immutable revision'); END;
CREATE TRIGGER observation_revisions_immutable_delete BEFORE DELETE ON observation_revisions BEGIN SELECT RAISE(ABORT,'immutable revision'); END;
PRAGMA user_version=3;
CREATE TABLE frame_provenance(transfer TEXT PRIMARY KEY, evidence TEXT NOT NULL);
CREATE TRIGGER frame_provenance_immutable_update BEFORE UPDATE ON frame_provenance BEGIN SELECT RAISE(ABORT,'immutable frame provenance'); END;
CREATE TRIGGER frame_provenance_immutable_delete BEFORE DELETE ON frame_provenance BEGIN SELECT RAISE(ABORT,'immutable frame provenance'); END;
