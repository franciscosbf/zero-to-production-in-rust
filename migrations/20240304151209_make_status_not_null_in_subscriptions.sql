BEGIN;
  UPDATE subscriptions SET status = 'confirmed' WHERE status IS NOT NULL;
  ALTER TABLE subscriptions ALTER COLUMN status SET NOT NULL;
COMMIT;
