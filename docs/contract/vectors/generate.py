import json,base64,hmac,hashlib
from pathlib import Path
root=Path('docs/contract/vectors')
def code(value): return dict(namespace='synthetic',value=value)
receipt_id='019ab000-0000-7000-8000-000000000010'
part=dict(test=code('TEST'),values=[dict(original='<0.10',parsed=dict(numeric=dict(number='0.10',comparator='less')))],units=dict(original='mg/L',mapping=dict(source='mg/L',target='mg/L')),dilution=dict(original='1',factor='1',applied='unknown'),flags=[],reference_range='0-1',comments=[],qc_class='patient',source_status='F',status='final',source_path=[dict(occurrence=1,kind='OBX',sequence='1',status='F')],times=[dict(role='analysis',time=dict(original='202611151159',parsed='2026-11-15T11:59:00',offset=None,precision='minute'))],received_at='2026-11-15T11:59:59Z')
result=dict(receipt=receipt_id,sample=dict(id='000Ab12',source='scanned_by_machine',position=dict(kind='rack',carrier='A',slot='03')),processing='production',identity=dict(resolved=dict(observation=dict(code=code('TEST'),sub_id='',run='',order=code('order-1')),revision=0)),parts=[part])
delivery=dict(contract_version=1,delivery_id='019ab000-0000-7000-8000-000000000001',created_at='2026-11-15T12:00:00Z',source=dict(analyzer='synthetic-1',connection='bench-1'),receipt=dict(id=receipt_id,received_at='2026-11-15T11:59:59Z',raw_sha256='00'*32,driver=code('1'),config_version='1',config_sha256='01'*32),results=[result])

def write(name,obj): (root/name).write_text(json.dumps(obj,indent=2)+'\n',newline='\n')
write('delivery.json',delivery)
import copy
for name in ['unresolved-identity','missing-sample','multi-part-coded','withdrawn']:
    doc=copy.deepcopy(delivery)
    r=doc['results'][0]
    if name=='unresolved-identity': r['identity']={'unresolved':'Source lacks run identity'}
    if name=='missing-sample': r['sample']=None
    if name=='multi-part-coded':
        r['parts'][0]['values']=[dict(original='POS',parsed=dict(coded=dict(code=code('POS'),text='Positive')))]*2
        r['parts'].append(copy.deepcopy(r['parts'][0]))
        r['parts'][1]['source_path'][0]['sequence']='2'
    if name=='withdrawn':
        r['parts'][0]['status']='withdrawn'
        r['parts'][0]['source_status']='D'
    write(name+'.json',doc)

# Protocol-neutral decoded text beside unchanged synthetic source spelling.
for name, original, decoded, parsed in [
    ("decoded-text", r"A\F\B", "A|B", "text"),
    ("unsupported-formatting", r"before\H\marked\N\after", None,
     dict(unsupported=dict(source_type="TX", reason="Highlighting is not interpreted"))),
]:
    doc=copy.deepcopy(delivery)
    value=dict(original=original,parsed=parsed)
    if decoded is not None: value["decoded_text"]=decoded
    doc["results"][0]["parts"][0]["values"]=[value]
    write(name+".json",doc)

event=dict(contract_version=1,delivery_id='019ab000-0000-7000-8000-000000000002',created_at='2026-11-15T12:00:00Z',source=delivery['source'],order_id='order-1',order_version=1,state='uncertain',occurred_at='2026-11-15T11:59:59Z',source_status='timeout',reason='No answer')
write('event.json',event)
write('order.json',dict(id='order-1',version=1,state='requested',sample_id='000Ab12',tests=[code('TEST')],priority='routine',target_analyzers=None,expires_at=None,tombstone=False))
body=json.dumps(delivery,separators=(',',':'))
key=bytes(range(32)); secret='whsec_'+base64.b64encode(key).decode(); timestamp='1794744000'
signature='v1,'+base64.b64encode(hmac.new(key,(delivery['delivery_id']+'.'+timestamp+'.'+body).encode(),hashlib.sha256).digest()).decode()
write('signature.json',dict(secret=secret,id=delivery['delivery_id'],timestamp=timestamp,body=body,signature=signature,digest=hashlib.sha256(body.encode()).hexdigest()))
write('responses.json',[dict(status=s,body=dict(contract_version=1,status='accepted') if s==200 else dict(contract_version=1,error='example',detail='Synthetic rejection'),expected=e) for s,e in [(200,'done'),(400,'parked'),(422,'parked'),(401,'pause'),(403,'pause'),(408,'retry'),(409,'conflict'),(429,'retry'),(500,'retry'),(302,'pause'),(404,'pause'),(413,'pause'),(418,'parked')]])
bad=dict(delivery);bad['contract_version']=2
write('invalid-version.json',bad)

write('invalid-signature.json',dict(id=delivery['delivery_id'],timestamp=timestamp,body=body,signature='v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=',expected='reject'))
write('accepted.json',dict(contract_version=1,status='accepted'))
write('duplicate.json',dict(contract_version=1,status='duplicate'))

# Optional general app-order context; keep the minimal order vector unchanged.
context_order=json.loads((root/'order.json').read_text())
context_order.update(position=dict(kind='tray',carrier='A',slot='03'),specimen_type=code('SERUM'),patient=dict(id=code('000PatA')))
write('order-context.json',context_order)

# Vectors for the contract checker and for each v1 rule that had none. Each
# block names the v1.md section it covers; the checker crate runs them.
import calendar

# v1 "HTTP and authentication": unknown fields are ignored at every level.
doc=copy.deepcopy(delivery)
doc['future_envelope_field']=dict(kept=True)
doc['receipt']['future_receipt_field']='ignored'
doc['results'][0]['future_result_field']=[1,2]
doc['results'][0]['parts'][0]['future_part_field']=None
write('unknown-fields.json',doc)

# v1 "POST /results": each hierarchy node carries its one-based occurrence.
doc=copy.deepcopy(delivery)
doc['results'][0]['parts'][0]['source_path']=[dict(occurrence=1,kind='MSH',sequence=None,status=None),dict(occurrence=3,kind='OBR',sequence='1',status='F'),dict(occurrence=5,kind='OBX',sequence='2',status='F')]
write('source-path-occurrence.json',doc)

# v1 "POST /results": documents a receiver refuses with 400 or 422 and an error body.
def invalid(name,rule,change):
    doc=copy.deepcopy(delivery)
    change(doc)
    return dict(name=name,rule=rule,document=doc)
def drop_occurrence(doc): del doc['results'][0]['parts'][0]['source_path'][0]['occurrence']
def other_receipt(doc): doc['results'][0]['receipt']='019ab000-0000-7000-8000-000000000099'
def unit_mismatch(doc): doc['results'][0]['parts'][0]['units']['mapping']['source']='mg/dL'
def partial_text(doc): doc['results'][0]['parts'][0]['values']=[dict(original='before\\H\\marked\\N\\after',decoded_text='partially interpreted',parsed=dict(unsupported=dict(source_type='TX',reason='Highlighting is not interpreted')))]
def no_results(doc): doc['results']=[]
def not_decimal(doc): doc['results'][0]['parts'][0]['values'][0]['parsed']['numeric']['number']='1e3'
def bare_test(doc): doc['results'][0]['parts'][0]['test']='TEST'
write('invalid-documents.json',[
    invalid('missing-occurrence','Each hierarchy node has required occurrence',drop_occurrence),
    invalid('receipt-mismatch','Replacement unit receipt matches envelope receipt.id',other_receipt),
    invalid('unit-mapping-mismatch','Unit mapping source is exactly equal to original',unit_mismatch),
    invalid('partial-decoded-text','decoded_text is absent for unsupported values',partial_text),
    invalid('empty-results','results holds 1 to 1000 replacement units',no_results),
    invalid('exponent-decimal','Exact decimals have no exponent',not_decimal),
    invalid('bare-test-code','Test codes are namespaced identifiers',bare_test),
])

# v1 "HTTP and authentication": requests a receiver refuses with 401 or 403.
write('authentication.json',[
    dict(name='wrong-api-key',request='POST /results',change='Bearer key differs from the destination key',expected=[401,403]),
    dict(name='missing-api-key',request='POST /results',change='No Authorization header',expected=[401,403]),
    dict(name='invalid-signature',request='POST /results',change='webhook-signature from invalid-signature.json',expected=[401,403]),
    dict(name='missing-signature',request='POST /results',change='No webhook-signature header',expected=[401,403]),
    dict(name='stale-timestamp',request='POST /results',change='Correct signature over a timestamp 600 seconds old',expected=[401,403]),
    dict(name='future-timestamp',request='POST /results',change='Correct signature over a timestamp 600 seconds ahead',expected=[401,403]),
    dict(name='orders-wrong-api-key',request='GET /orders',change='Bearer key differs from the destination key',expected=[401,403]),
    dict(name='orders-missing-api-key',request='GET /orders',change='No Authorization header',expected=[401,403]),
])

# v1 "HTTP and authentication": the timestamp window is 300 seconds either way.
write('signature-window.json',[dict(offset_seconds=o,expected='accept' if abs(o)<=300 else 'reject') for o in [-301,-300,0,300,301]])

# v1 "HTTP and authentication": any matching candidate signature is accepted (rotation).
other_key=bytes(range(32,64)); other_secret='whsec_'+base64.b64encode(other_key).decode()
other='v1,'+base64.b64encode(hmac.new(other_key,(delivery['delivery_id']+'.'+timestamp+'.'+body).encode(),hashlib.sha256).digest()).decode()
write('rotated-signature.json',dict(uses='signature.json',other_secret=other_secret,signature_header=other+' '+signature,expected='accept'))

# v1 "Delivery responses and retries": one delivery ID, its retries and conflicts.
write('idempotency.json',[
    dict(step='first',endpoint='results',document='delivery.json',change=None,expected_status=200,expected_body=dict(contract_version=1,status='accepted')),
    dict(step='retry',endpoint='results',document='delivery.json',change=None,expected_status=200,expected_body=dict(contract_version=1,status='duplicate')),
    dict(step='changed-content',endpoint='results',document='delivery.json',change=dict(created_at='2026-11-15T12:00:01Z'),expected_status=409,expected_body=None),
    dict(step='other-endpoint',endpoint='order-events',document='event.json',change=None,expected_status=409,expected_body=None),
])

# v1 "Delivery responses and retries": Retry-After seconds or IMF-fixdate, honoured up to the cap;
# a value longer than 64 characters counts as malformed (RFC 9110 §5.4 lets a client ignore it).
now=calendar.timegm((2026,11,18,12,0,0))
write('retry-after.json',dict(cap_seconds=60,now=now,cases=[
    dict(value=v,requested_seconds=r,wait_at_least=None if r is None else min(r,60),wait_at_most=60) for v,r in [
        ('30',30),('120',120),('0',0),
        ('Wed, 18 Nov 2026 12:00:45 GMT',45),('Wed, 18 Nov 2026 11:59:00 GMT',0),('Wed, 18 Nov 2026 12:05:00 GMT',300),
        ('-5',None),('1.5',None),('soon',None),('',None),('18 Nov 2026 12:00:45 GMT',None),('Wed, 18 Nov 2026 12:00:45 UTC',None),
        ('0'*62+'45',45),('0'*63+'45',None),
    ]]))

# v1 "Delivery responses and retries": every 3xx pauses, only 200 is done, other 2xx park.
rows=json.loads((root/'responses.json').read_text())
error=dict(contract_version=1,error='example',detail='Synthetic rejection')
rows+=[dict(status=s,body=b,expected=e) for s,b,e in [
    (301,error,'pause'),(307,error,'pause'),(308,error,'pause'),(204,error,'parked'),
    (200,{},'parked'),(502,error,'retry'),(503,error,'retry'),(504,error,'retry')]]
write('responses.json',rows)

# v1 "GET /orders": a client keeps the higher version and one content per version.
def offered(version,state):
    order=json.loads((root/'order.json').read_text())
    order.update(version=version,state=state)
    return order
write('order-versions.json',[
    dict(step='first',offer=offered(2,'requested'),kept_version=2,kept_state='requested',parked=False),
    dict(step='same-version-same-content',offer=offered(2,'requested'),kept_version=2,kept_state='requested',parked=False),
    dict(step='lower-version',offer=offered(1,'requested'),kept_version=2,kept_state='requested',parked=True),
    dict(step='same-version-different-content',offer=offered(2,'cancel_requested'),kept_version=2,kept_state='requested',parked=True),
    dict(step='higher-version',offer=offered(3,'cancel_requested'),kept_version=3,kept_state='cancel_requested',parked=False),
])

# v1 "GET /orders": only a chain that reaches complete refreshes freshness, from its start.
write('freshness.json',dict(freshness_seconds=30,chains=[
    dict(chain='snapshot',started=1000,finished=1004,pages=[False,True],fresh_from=1000,trusted_at_finish=True),
    dict(chain='stops-early',started=1100,finished=1103,pages=[False],fresh_from=1000,trusted_at_finish=False),
    dict(chain='slow-changes-since',started=1200,finished=1250,pages=[True],fresh_from=1200,trusted_at_finish=False),
    dict(chain='changes-since',started=1300,finished=1301,pages=[True],fresh_from=1300,trusted_at_finish=True),
]))

# v1 "GET /orders": orders an app loads before running the checker with --seed-file.
def seeded(id,version,state,sample,tests,priority,tombstone=False,targets=None,expires=None):
    return dict(id=id,version=version,state=state,sample_id=sample,tests=[code(t) for t in tests],priority=priority,target_analyzers=targets,expires_at=expires,tombstone=tombstone)
write('seed-orders.json',[
    seeded('seed-order-1',1,'requested','000Ab12',['TEST'],'routine'),
    seeded('seed-order-2',2,'requested','00Cd034',['TEST','TEST2'],'stat',targets=['synthetic-1'],expires='2026-12-31T23:59:59Z'),
    seeded('seed-order-3',3,'cancel_requested','0000077',['TEST'],'urgent'),
    seeded('seed-order-4',2,'cancel_confirmed','000Ef56',[],'routine',tombstone=True),
])

# v1 limits in one place, for checkers in any language.
write('limits.json',dict(max_document_bytes=1048576,max_results_per_delivery=1000,max_orders_per_page=100,default_page_limit=100,max_cursor_bytes=256,max_tests_per_order=100,max_seed_orders=100,cursor_lifetime_seconds=3600,signature_tolerance_seconds=300,retry_after_cap_seconds=60))
