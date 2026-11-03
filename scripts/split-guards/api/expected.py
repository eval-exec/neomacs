#!/usr/bin/env python3
"""Derive PC1 expected signatures from actual previously-private baseline nodes.

This is NOT a public-surface pass and grants no exception/guard approval.
"""
import argparse,csv,json
from pathlib import Path
from surface import Graph,Ref,Refusal,load_maps,require

def main():
    p=argparse.ArgumentParser();p.add_argument('--baseline',required=True);p.add_argument('--hot-table',required=True);p.add_argument('--path-map',required=True);p.add_argument('--out',required=True)
    a=p.parse_args();out=Path(a.out);require(not out.exists(),'preserve existing evidence');out.mkdir(parents=True)
    g=Graph([a.baseline],load_maps(a.path_map));table=list(csv.DictReader(open(a.hot_table),delimiter='\t'));require(table,'empty approved-method input')
    rows=[];seen=set()
    for row in table:
        owner=row['canonical_owner'];method=row['method'];require((owner,method) not in seen,'duplicate owner/method');seen.add((owner,method))
        candidates=[Ref(0,id) for id,item in g.docs[0]['index'].items() if id in g.docs[0]['paths'] and g.path(Ref(0,id))==owner and g.inner(item)[0] in {'struct','enum','union'}]
        require(len(candidates)==1,'canonical owner must match exactly one baseline nominal: '+owner)
        target=candidates[0];_,body=g.inner(g.item(target));found=[]
        for id in body.get('impls',[]):
            ir=Ref(target.doc,str(id));_,impl=g.inner(g.item(ir))
            if impl.get('trait') is not None:continue
            context=g.norm({k:impl[k] for k in ['generics','for','trait','is_unsafe','is_negative','is_synthetic','blanket_impl'] if k in impl},ir.doc,False)
            for member in impl['items']:
                mr=Ref(ir.doc,str(member));mi=g.item(mr)
                if mi.get('name')!=method:continue
                require(g.inner(mi)[0]=='function','expected method is not a function')
                require(mi['visibility']!='public','exception already public in baseline: '+owner+'::'+method)
                key=owner+'::'+method;g.member(mr,key,'inherent_method',owner,context)
                spec=next(x for x in g.rows.values() if x['path']==key)
                found.append({'canonical_owner':owner,'method':method,'baseline_visibility':mi['visibility'],'expected_candidate_signature_sha256':spec['signature_sha256'],'baseline_definition':mi.get('span'),'source_table_row':row})
        require(len(found)==1,'expected exactly one inherent private baseline signature: '+owner+'::'+method);rows+=found
    receipt={'status':'EXPECTED_SIGNATURES_ONLY_NOT_PUBLIC_SURFACE_APPROVAL','inputs':g.inputs,'path_map':load_maps(a.path_map),'methods':rows}
    (out/'expected.json').write_text(json.dumps(receipt,indent=2)+'\n')
    with (out/'expected.tsv').open('w') as f:
        w=csv.DictWriter(f,fieldnames=['canonical_owner','method','baseline_visibility','expected_candidate_signature_sha256'],delimiter='\t');w.writeheader()
        for row in rows:w.writerow({k:row[k] for k in w.fieldnames})
    print(json.dumps({'status':receipt['status'],'method_count':len(rows)}))
if __name__=='__main__':main()
