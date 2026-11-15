import json,base64,hmac,hashlib
from pathlib import Path
root=Path('docs/contract/vectors')
delivery=dict(contract_version=1,delivery_id='019ab000-0000-7000-8000-000000000001',created_at='2026-11-15T12:00:00Z',source=dict(analyzer='synthetic-1',connection='bench-1'),receipt=dict(id='receipt-1',received_at='2026-11-15T11:59:59Z',raw_sha256='00'*32),sample=dict(id='000Ab12',id_source='barcode',position=dict(rack='A',slot='03')),results=[dict(identity=dict(status='resolved',namespace='synthetic',id='observation-1',revision=1),test='TEST',source_status='F',summary_status='final',value=dict(text='<0.10',decimal=dict(number='0.10',comparator='lt')),original_unit='mg/L',ucum_unit='mg/L',dilution=dict(factor='1',applied='unknown'),flags=[],reference_range='0-1',qc_class='patient',processing_mode='production',measured_at=dict(text='202611151159',parsed='2026-11-15T11:59:00Z'),comments=[])])
def write(name,obj): (root/name).write_text(json.dumps(obj,indent=2)+'\n')
write('delivery.json',delivery)
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
