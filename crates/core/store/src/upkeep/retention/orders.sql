SELECT EXISTS(SELECT 1 FROM machine_orders m WHERE m.id=?1
 AND NOT EXISTS(SELECT 1 FROM orders o WHERE o.destination=m.destination AND o.id=m.order_id
  AND CASE WHEN json_valid(json_extract(o.evidence,'$.document')) THEN
   json_extract(json_extract(o.evidence,'$.document'),'$.version') IS NULL OR
   (CAST(json_extract(json_extract(o.evidence,'$.document'),'$.version') AS TEXT)!=m.version AND NOT EXISTS(SELECT 1 FROM machine_order_history h WHERE h.machine_order=m.id AND h.cancel_version=CAST(json_extract(json_extract(o.evidence,'$.document'),'$.version') AS TEXT)))
  ELSE 1 END)
 AND m.state IN ('Done','Rejected','CancelConfirmed') AND m.updated_at<=?2
 AND NOT EXISTS(SELECT 1 FROM machine_order_items i WHERE i.machine_order=m.id AND i.state NOT IN ('Done','Rejected','CancelConfirmed'))
 AND NOT EXISTS(SELECT 1 FROM machine_order_item_history h WHERE h.machine_order=m.id AND (json_extract(h.evidence,'$.updated_at') IS NULL OR json_extract(h.evidence,'$.updated_at')>?2))
 AND NOT EXISTS(SELECT 1 FROM retention_holds h WHERE h.kind='order' AND h.active=1 AND json_extract(h.target,'$[0]')=m.destination AND json_extract(h.target,'$[1]')=m.order_id)
 AND NOT EXISTS(SELECT 1 FROM poll_intents p WHERE p.machine=m.machine AND p.state!='confirmed')
 AND NOT EXISTS(SELECT 1 FROM parked p,json_each(p.evidence,'$.targets') t WHERE t.value=m.order_id OR t.value=CAST(m.id AS TEXT))
 AND NOT EXISTS(SELECT 1 FROM outbox o WHERE o.kind='order_event' AND o.destination=m.destination AND o.order_id=m.order_id AND (o.state!='done' OR NOT EXISTS(SELECT 1 FROM attempts a WHERE a.delivery=o.id AND a.outcome='"Done"') OR (SELECT max(a.time) FROM attempts a WHERE a.delivery=o.id AND a.outcome='"Done"')>?2))
)
