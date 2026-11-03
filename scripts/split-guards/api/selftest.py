#!/usr/bin/env python3
"""Offline rustdoc-format57 synthetic controls; never executes Rust/product code."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from surface import Graph, Refusal, compare

BASE=Path(__file__).resolve().parent

def fixture(public=False, trait_export=False):
    index={};paths={}
    def add(i,name,kind,body,visibility='public',path=None):
        index[str(i)]={'id':i,'crate_id':0,'name':name,'visibility':visibility,'attrs':[],'deprecation':None,'inner':{kind:body}}
        if path:paths[str(i)]={'crate_id':0,'path':path.split('::'),'kind':kind}
    def fn(i,name,visibility,output=None):
        add(i,name,'function',{'sig':{'inputs':[['self',{'borrowed_ref':{'lifetime':None,'is_mutable':False,'type':{'generic':'Self'}}}]],'output':output,'is_c_variadic':False},'generics':{'params':[],'where_predicates':[]},'header':{'is_const':False,'is_unsafe':False,'is_async':False,'abi':'Rust'},'has_body':True},visibility)
    add(0,'demo','module',{'is_crate':True,'items':[1,2,3]+([8] if trait_export else []),'is_stripped':False},path='demo')
    add(1,'hidden','module',{'is_crate':False,'items':[4,7],'is_stripped':False},'default',path='demo::hidden')
    add(2,'Value','use',{'source':'hidden::TaggedValue','name':'Value','id':4,'is_glob':False})
    add(3,'Alias','type_alias',{'type':{'resolved_path':{'path':'hidden::TaggedValue','id':4,'args':None}},'generics':{'params':[],'where_predicates':[]}},path='demo::Alias')
    add(4,'TaggedValue','struct',{'kind':'unit','generics':{'params':[],'where_predicates':[]},'impls':[5,9]},path='demo::hidden::TaggedValue')
    add(5,None,'impl',{'is_unsafe':False,'generics':{'params':[],'where_predicates':[]},'provided_trait_methods':[],'trait':None,'for':{'resolved_path':{'path':'TaggedValue','id':4,'args':None}},'items':[6,11],'is_negative':False,'is_synthetic':False,'blanket_impl':None})
    fn(6,'hot','public' if public else 'crate',{'primitive':'usize'})
    fn(11,'stable','public',{'primitive':'bool'})
    add(7,'CoreExt','trait',{'is_auto':False,'is_unsafe':False,'is_dyn_compatible':True,'items':[10],'generics':{'params':[],'where_predicates':[]},'bounds':[],'implementations':[9]},path='demo::hidden::CoreExt')
    add(8,'CoreExt','use',{'source':'hidden::CoreExt','name':'CoreExt','id':7,'is_glob':False})
    add(9,None,'impl',{'is_unsafe':False,'generics':{'params':[],'where_predicates':[]},'provided_trait_methods':[],'trait':{'path':'CoreExt','id':7,'args':None},'for':{'resolved_path':{'path':'TaggedValue','id':4,'args':None}},'items':[12],'is_negative':False,'is_synthetic':False,'blanket_impl':None})
    fn(10,'internal','default',{'primitive':'usize'});fn(12,'internal','default',{'primitive':'usize'})
    return {'root':0,'crate_version':None,'includes_private':True,'index':index,'paths':paths,'external_crates':{},'target':{},'format_version':57}

class Controls(unittest.TestCase):
    def extract(self,doc,*deps,maps=()):
        # Files stay under the owned api-tool directory, removed synchronously.
        with tempfile.TemporaryDirectory(prefix='control-',dir=BASE) as d:
            files=[]
            for n,data in enumerate((doc,)+deps):
                p=Path(d)/f'{n}.json';p.write_text(json.dumps(data));files.append(p)
            return Graph(files,maps).extract()
    def accepted(self,b):
        groups={(r['owner'],r['method'],r['signature_sha256']) for r in b['rows'].values() if r['kind']=='inherent_method' and r['method']=='hot'}
        self.assertEqual(len(groups),1)
        owner,method,sha=next(iter(groups))
        return {(owner,method):{'owner':owner,'method':method,'candidate_signature_sha256':sha,'guard':'tracked-call-site-guard','protected_caller':'protected-chain','approval':'PC1'}}
    def test_identity(self):
        a=self.extract(fixture());self.assertTrue(compare(a,a,{})['passed'])
    def test_aliases_are_one_exact_method_group(self):
        a=self.extract(fixture());b=self.extract(fixture(True));r=compare(a,b,self.accepted(b))
        self.assertTrue(r['passed']);self.assertEqual(len(r['added']),2);self.assertEqual(len(r['actual_added_inherent_method_groups']),1)
        self.assertEqual({x['path'] for x in r['added']},{'demo::Value::hot','demo::Alias::hot'})
    def test_unaccepted_added_method_red(self):
        self.assertFalse(compare(self.extract(fixture()),self.extract(fixture(True)),{})['passed'])
    def test_exception_cannot_change_previous_private_signature(self):
        b=fixture(True);b['index']['6']['inner']['function']['sig']['output']={'primitive':'u8'}
        out=self.extract(b);self.assertFalse(compare(self.extract(fixture()),out,self.accepted(out))['passed'])
    def test_wrong_stale_and_extra_exception_red(self):
        a=self.extract(fixture());b=self.extract(fixture(True));acc=self.accepted(b)
        wrong=copy.deepcopy(acc);next(iter(wrong.values()))['candidate_signature_sha256']='0'*64
        self.assertFalse(compare(a,b,wrong)['passed']);self.assertFalse(compare(a,a,acc)['passed'])
        extra=copy.deepcopy(acc);extra[('demo::Other','unrelated')]=next(iter(acc.values()));self.assertFalse(compare(a,b,extra)['passed'])
    def test_public_method_removal_rename_and_signature_red(self):
        a=self.extract(fixture())
        for op in ('remove','rename','signature'):
            b=fixture()
            if op=='remove':b['index']['11']['visibility']='crate'
            if op=='rename':b['index']['11']['name']='renamed'
            if op=='signature':b['index']['11']['inner']['function']['sig']['output']={'primitive':'u32'}
            self.assertFalse(compare(a,self.extract(b),{})['passed'],op)
    def test_private_trait_is_not_exposed_by_public_type(self):
        a=self.extract(fixture());self.assertFalse(any('CoreExt' in x['path'] for x in a['rows'].values()))
        b=fixture();b['index']['12']['name']='changed_private_method'
        self.assertTrue(compare(a,self.extract(b),{})['passed'])
    def test_public_trait_reexport_and_methods_are_visible(self):
        a=self.extract(fixture());b=self.extract(fixture(trait_export=True))
        self.assertFalse(compare(a,b,{})['passed']);self.assertTrue(any(x['kind']=='trait_method' and x['method']=='internal' for x in b['rows'].values()))
    def test_alias_removal_and_added_alias_red(self):
        a=self.extract(fixture());b=fixture();b['index']['3']['visibility']='crate'
        self.assertFalse(compare(a,self.extract(b),{})['passed'])
    def test_missing_moved_definition_refuses(self):
        b=fixture();b['index']['2']['inner']['use']['id']=99;b['paths']['99']={'crate_id':1,'path':['neovm_value','tagged','value','TaggedValue'],'kind':'struct'}
        with self.assertRaises(Refusal):self.extract(b)
    def test_dependency_move_and_alias_closure(self):
        old=fixture();core=fixture();dep=fixture(True)
        dep['index']['0']['name']='neovm_value'
        for path in dep['paths'].values():path['path'][0]='neovm_value'
        for id in (4,5,6,7,9,10,11,12):del core['index'][str(id)]
        core['index']['1']['inner']['module']['items']=[]
        core['index']['2']['inner']['use']['id']=99
        core['index']['3']['inner']['type_alias']['type']['resolved_path']['id']=99
        core['paths']['99']={'crate_id':1,'path':['neovm_value','hidden','TaggedValue'],'kind':'struct'}
        for id in list(core['paths']):
            if id not in core['index'] and id!='99':del core['paths'][id]
        mapping=[('demo::hidden','neovm_value::hidden')]
        a=self.extract(old,maps=mapping);b=self.extract(core,dep,maps=mapping)
        result=compare(a,b,self.accepted(b));self.assertTrue(result['passed'],result['errors'])
    def test_trait_required_default_change_red(self):
        a=self.extract(fixture(trait_export=True));b=fixture(trait_export=True)
        b['index']['10']['inner']['function']['has_body']=False
        self.assertFalse(compare(a,self.extract(b),{})['passed'])
    def test_opaque_dependency_needs_exact_paired_evidence(self):
        a=self.extract(fixture());a['opaque_dependency_crates']=['display'];a['opaque_external_or_non_nominal_targets']=['display::Thing']
        self.assertFalse(compare(a,a,{})['passed'])
        equal={'baseline_sha256':'a'*64,'candidate_sha256':'a'*64}
        evidence={x:equal.copy() for x in ('toolchain','target','cargo_lock','features')};evidence['dependencies']={'display':equal.copy()}
        self.assertTrue(compare(a,a,{},evidence)['passed'])
        evidence['dependencies']['display']['candidate_sha256']='b'*64
        self.assertFalse(compare(a,a,{},evidence)['passed'])
    def test_non_nominal_alias_requires_builtin_identity_and_detects_signature_change(self):
        b=fixture();b['index']['3']['inner']['type_alias']['type']={'tuple':[{'primitive':'u8'}]}
        a=self.extract(b);self.assertIn('core',a['opaque_dependency_crates']);self.assertFalse(compare(a,a,{})['passed'])
        b['index']['3']['inner']['type_alias']['type']={'tuple':[{'primitive':'u16'}]}
        self.assertFalse(compare(a,self.extract(b),{})['passed'])
    def test_owned_trait_impl_on_builtin_alias_is_not_hidden(self):
        a=fixture(trait_export=True);a['index']['3']['inner']['type_alias']['type']={'primitive':'u8'}
        b=copy.deepcopy(a);b['index']['9']['inner']['impl']['for']={'primitive':'u8'}
        old=self.extract(a);new=self.extract(b)
        self.assertTrue(any(r['kind']=='owned_trait_impl' and r['owner'].startswith('@type::') for r in new['rows'].values()))
        self.assertFalse(compare(old,new,{})['passed'])
    def test_borrowed_alias_inherent_methods_and_mutability(self):
        old=fixture();old['index']['3']['inner']['type_alias']['type']={'borrowed_ref':{'lifetime':None,'is_mutable':False,'type':{'resolved_path':{'id':4,'path':'TaggedValue','args':None}}}}
        new=copy.deepcopy(old);new['index']['6']['visibility']='public'
        a=self.extract(old);b=self.extract(new)
        self.assertTrue(any(r['kind']=='deref_inherent_method' and r['path']=='demo::Alias::<deref>::hot' for r in b['rows'].values()))
        evidence={x:{'baseline_sha256':'a'*64,'candidate_sha256':'a'*64} for x in ('toolchain','target','cargo_lock','features')}
        evidence['dependencies']={'core':{'baseline_sha256':'a'*64,'candidate_sha256':'a'*64}}
        self.assertTrue(compare(a,b,self.accepted(b),evidence)['passed'])
        new['index']['6']['inner']['function']['sig']['inputs'][0][1]['borrowed_ref']['is_mutable']=True
        c=self.extract(new)
        self.assertFalse(any(r['path']=='demo::Alias::<deref>::hot' for r in c['rows'].values()))
    def test_unknown_schema_refuses(self):
        b=fixture();b['format_version']=58
        with self.assertRaises(Refusal):self.extract(b)

if __name__=='__main__':unittest.main(verbosity=2)
