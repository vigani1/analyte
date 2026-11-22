-- Step 18 to 19: the machine model each machine order was reserved for
-- (ADR 0033), so its order events can name the model without the current
-- program file. An interrupted order of a machine that was removed since then
-- must still become uncertain at restart and reach the lab app.
--
-- New orders: the engine passes the model of the machine's definition (its
-- `vendor/model` folder key) when it reserves the order, and the store writes
-- it in the same insert. A machine order is reserved once, so the value never
-- changes.
--
-- Orders already stored: nothing kept beside them says which definition their
-- machine had (the program file names the machine, not the model, and orders
-- keep no configuration copy), so they keep NULL. Their later order events are
-- parked for review with the reason instead of being sent with a guessed
-- model; the order's own lifecycle is recorded as before.
ALTER TABLE machine_orders ADD COLUMN model TEXT CHECK(
 model IS NULL OR (
  length(model) BETWEEN 3 AND 129
  AND model GLOB '?*/?*'
  AND model NOT GLOB '*/*/*'
  AND model NOT GLOB '*[^a-z0-9/-]*'
 )
);
PRAGMA user_version=19;
