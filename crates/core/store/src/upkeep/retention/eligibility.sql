SELECT EXISTS(
 SELECT 1 FROM receipts r WHERE r.id=?1
 AND EXISTS(SELECT 1 FROM interpretations i WHERE i.receipt=r.id)
 AND EXISTS(SELECT 1 FROM outbox o WHERE o.receipt=r.id)
 AND NOT EXISTS(SELECT 1 FROM interpretations i WHERE i.receipt=r.id AND (json_type(i.evidence,'$.outcome.Results') IS NOT 'array' OR json_array_length(i.evidence,'$.outcome.Results')=0))
 AND NOT EXISTS(
  SELECT 1 FROM interpretations i, json_each(i.evidence,'$.outcome.Results') result
  WHERE i.receipt=r.id AND (
   json_type(result.value,'$.identity.Resolved') IS NOT 'object'
   OR NOT EXISTS(SELECT 1 FROM outbox o WHERE o.receipt=r.id AND o.version=i.version
    AND json_extract(o.ordering_key,'$.observation')=json_extract(result.value,'$.identity.Resolved.observation'))))
 AND NOT EXISTS(SELECT 1 FROM outbox o WHERE o.receipt=r.id AND (o.state!='done'
  OR NOT EXISTS(SELECT 1 FROM attempts a WHERE a.delivery=o.id AND a.outcome='"Done"')
  OR (SELECT max(a.time) FROM attempts a WHERE a.delivery=o.id AND a.outcome='"Done"')>?2))
 -- Retain even resolved parking evidence: its protected audit may refer to the original.
 AND NOT EXISTS(SELECT 1 FROM parked p, json_each(p.evidence,'$.targets') target
  WHERE target.value=json_extract(r.metadata,'$.id') OR target.value IN
   (SELECT json_extract(o.id,'$') FROM outbox o WHERE o.receipt=r.id))
 AND NOT EXISTS(SELECT 1 FROM transfers t WHERE t.receipt=r.id AND t.state!='complete')
 -- An uncertain machine operation may lack a trustworthy receipt link; retain the machine's data.
 AND NOT EXISTS(SELECT 1 FROM poll_intents p WHERE p.machine=json_extract(r.metadata,'$.machine') AND p.state!='confirmed')
 AND NOT EXISTS(SELECT 1 FROM machine_orders m WHERE m.machine=json_extract(r.metadata,'$.machine') AND m.state NOT IN ('Done','Rejected','CancelConfirmed'))
 AND NOT EXISTS(SELECT 1 FROM machine_order_items item JOIN machine_orders m ON m.id=item.machine_order WHERE m.machine=json_extract(r.metadata,'$.machine') AND item.state NOT IN ('Done','Rejected','CancelConfirmed'))
 AND NOT EXISTS(SELECT 1 FROM retention_holds h WHERE h.active=1 AND
  ((h.kind='receipt' AND h.target=r.id) OR h.kind='order'))
)
