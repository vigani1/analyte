import json,base64,hmac,hashlib
from pathlib import Path
root=Path('docs/contract/vectors')
def code(value): return dict(namespace='synthetic',value=value)
receipt_id='019ab000-0000-7000-8000-000000000010'
part=dict(test=code('TEST'),values=[dict(original='<0.10',parsed=dict(numeric=dict(number='0.10',comparator='less')))],units=dict(original='mg/L',mapping=dict(source='mg/L',target='mg/L')),dilution=dict(factor='1',applied='unknown'),flags=[],reference_range='0-1',comments=[],qc_class='patient',source_status='F',status='final',source_path=[dict(kind='OBX',sequence='1',status='F')],times=[dict(role='analysis',time=dict(original='202611151159',parsed='2026-11-15T11:59:00',offset=None,precision='minute'))],received_at='2026-11-15T11:59:59Z')
result=dict(receipt=receipt_id,sample=dict(id='000Ab12',source='scanned_by_machine',position=dict(kind='rack',carrier='A',slot='03')),processing='production',identity=dict(resolved=dict(observation=dict(code=code('TEST'),sub_id='',order=code('order-1')),revision=0)),parts=[part])
delivery=dict(contract_version=1,delivery_id='019ab000-0000-7000-8000-000000000001',created_at='2026-11-15T12:00:00Z',source=dict(analyzer='synthetic-1',connection='bench-1'),receipt=dict(id=receipt_id,received_at='2026-11-15T11:59:59Z',raw_sha256='00'*32,driver=code('1'),config_version='1',config_sha256='01'*32),results=[result])

def write(name,obj): (root/name).write_text(json.dumps(obj,indent=2)+'\n')
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

event=dict(contract_version=1,delivery_id='019ab000-0000-7000-8000-000000000002',created_at='2026-11-15T12:00:00Z',source=delivery['source'],order_id='order-1',order_version=1,state='uncertain',occurred_at='2026-11-15T11:59:59Z',source_status='timeout',reason='No answer')
write('event.json',event)
write('order.json',dict(id='order-1',version=1,state='requested',sample_id='000Ab12',tests=['TEST'],priority='routine',target_analyzers=None,expires_at=None,tombstone=False))
body=json.dumps(delivery,separators=(',',':'))
key=bytes(range(32)); secret='whsec_'+base64.b64encode(key).decode(); timestamp='1794744000'
signature='v1,'+base64.b64encode(hmac.new(key,(delivery['delivery_id']+'.'+timestamp+'.'+body).encode(),hashlib.sha256).digest()).decode()
write('signature.json',dict(secret=secret,id=delivery['delivery_id'],timestamp=timestamp,body=body,signature=signature,digest=hashlib.sha256(body.encode()).hexdigest()))
write('responses.json',[dict(status=s,body=dict(contract_version=1,status='accepted') if s==200 else dict(contract_version=1,error='example',detail='Synthetic rejection'),expected=e) for s,e in [(200,'done'),(400,'parked'),(422,'parked'),(401,'pause'),(403,'pause'),(408,'retry'),(409,'conflict'),(429,'retry'),(500,'retry'),(302,'parked')]])
bad=dict(delivery);bad['contract_version']=2
write('invalid-version.json',bad)

write('invalid-signature.json',dict(id=delivery['delivery_id'],timestamp=timestamp,body=body,signature='v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=',expected='reject'))
write('accepted.json',dict(contract_version=1,status='accepted'))
write('duplicate.json',dict(contract_version=1,status='duplicate'))
