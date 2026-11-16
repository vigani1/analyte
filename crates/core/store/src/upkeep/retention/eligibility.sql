SELECT EXISTS(
 SELECT 1 FROM receipts r WHERE r.id=?1
 AND NOT EXISTS(SELECT 1 FROM awos_execution_history h WHERE h.receipt=r.id)
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
 AND NOT EXISTS(SELECT 1 FROM retention_holds h WHERE h.active=1 AND h.kind='receipt' AND h.target=r.id)
)
