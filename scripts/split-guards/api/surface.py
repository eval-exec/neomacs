#!/usr/bin/env python3
"""Rustdoc JSON format-57 public graph comparator. Python standard library only.

This is a source/API checker, not a Rust type checker or a contract/census pass.
All item IDs are document-local. No name-only method matching grants authority.
"""
import argparse
from collections import defaultdict, deque
import csv
from dataclasses import dataclass
import gzip
import hashlib
import json
from pathlib import Path
import re
import sys

SUPPORTED_FORMATS = {57}
NOMINAL = {'struct', 'enum', 'union', 'primitive'}
LEAF_KINDS = {'function', 'constant', 'static', 'macro', 'proc_macro',
              'assoc_const', 'assoc_type', 'foreign_type', 'extern_type', 'trait_alias'}
METHOD_KINDS = {'inherent_method', 'deref_inherent_method'}
TYPE_HEADS={'resolved_path','dyn_trait','generic','primitive','function_pointer','tuple','slice','array','impl_trait','infer','raw_pointer','borrowed_ref','qualified_path','pat'}

class Refusal(Exception):
    pass

def require(condition, message):
    if not condition:
        raise Refusal(message)

def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()

def read_doc(path):
    path = Path(path)
    raw = path.read_bytes()
    data = gzip.decompress(raw) if path.suffix == '.gz' else raw
    doc = json.loads(data)
    require(doc.get('format_version') in SUPPORTED_FORMATS, 'unsupported rustdoc format_version: ' + str(doc.get('format_version')))
    require(doc.get('includes_private') is True, 'private nodes required for alias/impl closure')
    require(isinstance(doc.get('index'), dict) and isinstance(doc.get('paths'), dict), 'missing index/paths')
    require(str(doc['root']) in doc['index'], 'missing root item')
    return doc, {'path':str(path.resolve()), 'sha256':hashlib.sha256(data).hexdigest(),
                 'transport_sha256':hashlib.sha256(raw).hexdigest(), 'format_version':doc['format_version']}

def producer_receipts(paths,inputs):
    require(len(paths)==len(inputs),'one producer receipt required for each JSON document')
    receipts=[]
    for path,info in zip(paths,inputs):
        p=Path(path);raw=p.read_bytes();row=json.loads(raw)
        require(row.get('rustdoc_json_sha256')==info['sha256'],'producer JSON SHA256 mismatch: '+str(path))
        require(row.get('format_version')==info['format_version'],'producer format_version mismatch')
        command=row.get('command',[])
        require(isinstance(command,list) and all(isinstance(x,str) for x in command),'producer command must be exact argv')
        require('--document-private-items' in command and '--document-hidden-items' in command,'incomplete producer command: BOTH --document-private-items and --document-hidden-items required')
        require(row.get('source_pin') and row.get('tools'),'producer needs source pin and toolchain receipt')
        receipts.append({'path':str(p.resolve()),'sha256':hashlib.sha256(raw).hexdigest(),'record':row})
    return receipts

@dataclass(frozen=True)
class Ref:
    doc: int
    id: str

class Graph:
    def __init__(self, documents, maps=()):
        self.docs=[]; self.inputs=[]; self.maps=maps
        for path in documents:
            d, receipt=read_doc(path); self.docs.append(d); self.inputs.append(receipt)
        self.names=[d['index'][str(d['root'])]['name'] for d in self.docs]
        require(len(set(self.names)) == len(self.names), 'duplicate crate JSON input')
        self.path_refs=defaultdict(list); self.fallback={}; self.opaque=set()
        for n,d in enumerate(self.docs):
            for key,summary in d['paths'].items():
                if key in d['index']:
                    self.path_refs[tuple(summary['path'])].append(Ref(n,key))
            self._local_paths(Ref(n,str(d['root'])), self.names[n], set())
        self.rows={}; self.specs={}; self.sigrefs=set(); self.public_traits=set(); self.private_methods={}; self.opaque_crates=set()
        self.exported={}; self.type_views=[]; self.method_cache={}; self.done_views=set(); self.exposed_forms=set(); self.exposed_nominals=set(); self.builtin_aliases={}; self.done_impls=set()

    def _local_paths(self, ref, path, seen):
        if ref in seen: return
        seen.add(ref); self.fallback.setdefault(ref,path)
        item=self.item(ref); kind,body=self.inner(item)
        if kind=='module':
            for id in body['items']:
                child=Ref(ref.doc,str(id)); obj=self.item(child)
                if obj.get('name'):
                    self._local_paths(child,path+'::'+obj['name'],seen)

    def normalize_path(self, path):
        matches=[(a,b) for a,b in self.maps if path==a or path.startswith(a+'::')]
        if not matches: return path
        a,b=max(matches,key=lambda pair:len(pair[0]))
        return b+path[len(a):]

    def item(self, ref):
        try: return self.docs[ref.doc]['index'][ref.id]
        except KeyError: raise Refusal('missing item definition '+str(ref))

    @staticmethod
    def inner(item):
        require(isinstance(item.get('inner'),dict) and len(item['inner'])==1, 'invalid item inner')
        return next(iter(item['inner'].items()))

    def summary(self, ref):
        return self.docs[ref.doc]['paths'].get(ref.id)

    def path(self, ref):
        summary=self.summary(ref)
        if summary: return self.normalize_path('::'.join(summary['path']))
        if ref in self.fallback: return self.normalize_path(self.fallback[ref])
        item=self.docs[ref.doc]['index'].get(ref.id)
        raise Refusal('no canonical path for ID '+str(ref)+' name='+str(item and item.get('name')))

    def resolve(self, ref, required=True):
        summary=self.summary(ref)
        if summary and summary['path'][0]!=self.names[ref.doc]:
            defining=[x for x in self.path_refs.get(tuple(summary['path']),[]) if self.names[x.doc]==summary['path'][0]]
            if defining:
                require(len(defining)==1,'ambiguous defining dependency '+str(summary['path']))
                return defining[0]
        if ref.id in self.docs[ref.doc]['index']: return ref
        require(summary is not None, 'unresolved ID without path summary '+str(ref))
        matches=self.path_refs.get(tuple(summary['path']),[])
        preferred=[x for x in matches if self.names[x.doc]==summary['path'][0]]
        if preferred: matches=preferred
        if matches:
            # A defining-crate document wins over inlined copies.
            require(len({(self.names[x.doc],self.path(x)) for x in matches})==1,
                    'ambiguous dependency definition '+str(summary['path']))
            return matches[0]
        self.opaque.add('::'.join(summary['path']))
        if required:
            raise Refusal('dependency JSON required for exported nominal/trait/module '+ '::'.join(summary['path']))
        return None

    def norm(self, value, doc, collect=True):
        if isinstance(value,list): return [self.norm(x,doc,collect) for x in value]
        if not isinstance(value,dict): return value
        # rustdoc Path includes id and path/name; IDs themselves have no cross-build meaning.
        if 'id' in value and ('path' in value or 'name' in value):
            ref=Ref(doc,str(value['id']))
            try:path=self.path(ref)
            except Refusal as error:raise Refusal(str(error)+' Path='+repr(value))
            if collect:
                target=self.resolve(ref,False)
                if target is None:
                    summary=self.summary(ref)
                    require(summary and summary['path'][0] not in {self.names[0],'neovm_value'},'missing moved/local signature definition '+path)
                    self.opaque_crates.add(summary['path'][0])
                if target:
                    kind,_=self.inner(self.item(target))
                    if kind in NOMINAL | {'trait','type_alias'}: self.sigrefs.add(target)
            return {k:self.norm(v,doc,collect) for k,v in value.items() if k not in {'id','path','name'}} | {'resolved':path}
        result={k:self.norm(v,doc,collect) for k,v in value.items()}
        if collect and len(value)==1 and next(iter(value)) in TYPE_HEADS:self.exposed_forms.add(digest(result))
        return result

    def common(self, item):
        # Documentation/link/source-location noise is excluded. Hidden public API is not excluded.
        attrs=[]
        for attr in item.get('attrs',[]):
            if isinstance(attr,str) and re.match(r'#\[doc\(',attr): continue
            attrs.append(attr)
        return {'attrs':attrs,'deprecation':item.get('deprecation')}

    def add(self, kind, path, spec, owner='', method='', overload=''):
        key=kind+'|'+path+('|'+overload if overload else '')
        sha=digest(spec); row={'kind':kind,'path':path,'owner':owner,'method':method,'signature_sha256':sha}
        if key in self.rows:
            require(self.rows[key]==row,'ambiguous API key '+key)
        self.rows[key]=row; self.specs.setdefault(sha,spec)

    def exported_walk(self, ref, path, ancestry=()):
        target=self.resolve(ref,False)
        if target is None:
            summary=self.summary(ref); require(summary is not None,'missing exported path summary')
            crate=summary['path'][0]
            require(crate not in {self.names[0],'neovm_value'},'missing moved/local exported definition '+self.path(ref))
            self.opaque_crates.add(crate)
            self.add('external_export',path,{'target':self.path(ref),'kind':summary['kind']})
            return
        item=self.item(target); kind,body=self.inner(item)
        require(target not in ancestry, 'cyclic public module/re-export expansion '+path)
        if kind=='use':
            require(body.get('id') is not None,'unresolved public use '+path+' source='+body.get('source',''))
            other=Ref(target.doc,str(body['id']))
            if body['is_glob']:
                other=self.resolve(other); k,b=self.inner(self.item(other))
                require(k=='module','non-module public glob '+path)
                self.module_walk(other,path.rsplit('::',1)[0],ancestry+(target,))
            else: self.exported_walk(other,path,ancestry+(target,))
            return
        self.exported[path]=target
        if kind=='module':
            self.add('module',path,self.common(item))
            self.module_walk(target,path,ancestry+(target,)); return
        if kind=='trait': self.public_traits.add(target)
        self.describe(target,path)
        if kind in NOMINAL | {'type_alias'}: self.type_views.append((target,path))

    def module_walk(self, ref, path, ancestry):
        _,body=self.inner(self.item(ref))
        for id in body['items']:
            child=Ref(ref.doc,str(id)); item=self.item(child)
            if item['visibility']!='public': continue
            kind,payload=self.inner(item)
            name=payload['name'] if kind=='use' else item.get('name')
            require(name is not None,'unnamed public module child '+str(child))
            self.exported_walk(child,path+'::'+name,ancestry)

    def describe(self, ref, path):
        item=self.item(ref);kind,body=self.inner(item);spec=self.common(item)
        if kind in NOMINAL:
            spec.update(self.norm({k:v for k,v in body.items() if k not in {'impls','kind','fields','variants'}},ref.doc))
            if kind=='struct':
                form=body['kind']; spec['form']=next(iter(form)) if isinstance(form,dict) else form
                fields=[]
                if isinstance(form,dict):
                    fields=form.get('plain',{}).get('fields',[]) if 'plain' in form else form.get('tuple',[])
                spec['field_slots']=[id is not None and self.item(Ref(ref.doc,str(id)))['visibility']=='public' for id in fields]
                self.fields(ref,fields,path,False)
            elif kind=='union': self.fields(ref,body['fields'],path,False)
            elif kind=='enum':
                for id in body['variants']:
                    variant=Ref(ref.doc,str(id)); vi=self.item(variant);vk,vb=self.inner(vi);require(vk=='variant','invalid enum variant')
                    vp=path+'::'+vi['name'];form=vb['kind']; shape=next(iter(form)) if isinstance(form,dict) else form
                    self.add('variant',vp,self.common(vi)|{'shape':shape,'discriminant':vb['discriminant']})
                    fields=form.get('tuple',[]) if isinstance(form,dict) and 'tuple' in form else form.get('struct',{}).get('fields',[]) if isinstance(form,dict) else []
                    self.fields(variant,fields,vp,True)
            self.add(kind,path,spec); return
        if kind=='trait':
            spec.update(self.norm({k:v for k,v in body.items() if k not in {'items','implementations'}},ref.doc))
            self.add(kind,path,spec)
            for id in body['items']:
                child=Ref(ref.doc,str(id)); ci=self.item(child)
                if ci['visibility'] in ('public','default'): self.member(child,path+'::'+ci['name'],'trait_member')
            return
        if kind=='type_alias':
            self.add(kind,path,spec|self.norm(body,ref.doc)); return
        require(kind in LEAF_KINDS,'unsupported public item kind '+kind+' at '+path)
        payload=body
        if kind=='function': payload={k:v for k,v in body.items() if k!='has_body'}
        self.add(kind,path,spec|{'definition':self.norm(payload,ref.doc)})

    def fields(self, parent, ids, path, enum):
        for position,id in enumerate(ids):
            if id is None: continue
            ref=Ref(parent.doc,str(id)); item=self.item(ref);kind,body=self.inner(item)
            require(kind=='struct_field','unknown field shape')
            if enum or item['visibility']=='public':
                self.add('field',path+'::'+(item.get('name') or str(position)),self.common(item)|{'type':self.norm(body,ref.doc)})

    def member(self, ref, path, kind, owner='', context=None, overload=''):
        item=self.item(ref);ik,body=self.inner(item)
        require(ik in {'function','assoc_const','assoc_type'},'unsupported associated item '+ik)
        payload={k:v for k,v in body.items() if k!='has_body' or kind=='trait_member'} if ik=='function' else body
        spec=self.common(item)|{'definition':self.norm(payload,ref.doc)}
        if context is not None: spec['impl']=context
        self.add(kind if ik=='function' else kind+'_'+ik,path,spec,owner,item['name'],overload)

    def alias_target(self, ref, seen=()):
        require(ref not in seen,'cyclic type alias '+str(ref))
        item=self.item(ref);kind,body=self.inner(item)
        if kind!='type_alias': return ref
        ty=body['type']
        if 'resolved_path' not in ty:
            # Builtin impls are conditional on the exact paired core/sysroot identity.
            # Core-owned public trait impls are collected independently below.
            normalized=self.norm(ty,ref.doc)
            self.builtin_aliases[self.path(ref)]=normalized;self.opaque_crates.add('core')
            return None
        other=Ref(ref.doc,str(ty['resolved_path']['id'])); other=self.resolve(other,False)
        if other is None:
            # This cannot hide moved definitions: neovm_value must always be supplied.
            summary=self.summary(Ref(ref.doc,str(ty['resolved_path']['id'])))
            require(summary and summary['path'][0] not in self.names[:1]+['neovm_value'], 'missing moved/local alias definition')
            return None
        return self.alias_target(other,seen+(ref,))

    def trait_visible(self, ref):
        target=self.resolve(ref,False)
        if target:
            if self.path(target) in {self.path(x) for x in self.public_traits}: return True
            item=self.item(target)
            # Local/unexported or private moving-crate traits do not leak through public types.
            if self.names[target.doc] in {self.names[0],'neovm_value'}: return False
            return item['visibility']=='public'
        summary=self.summary(ref)
        if summary: self.opaque_crates.add(summary['path'][0])
        return bool(summary and summary['path'][0] not in {self.names[0],'neovm_value'})

    def methods(self, ref, path, deref=False, mutable=True, ancestry=()):
        target=self.alias_target(ref)
        if target is None:
            item=self.item(ref);kind,body=self.inner(item)
            ty=body.get('type',{}) if kind=='type_alias' else {}
            writable=mutable;borrowed=False
            while 'borrowed_ref' in ty:
                borrowed=True;writable=writable and ty['borrowed_ref']['is_mutable'];ty=ty['borrowed_ref']['type']
            if borrowed and 'resolved_path' in ty:
                other=self.resolve(Ref(ref.doc,str(ty['resolved_path']['id'])),False)
                if other:self.methods(other,path+'::<deref>',True,writable,ancestry)
            return
        require(target not in ancestry,'cyclic deref target '+path)
        token=(target,path,deref,mutable)
        if token in self.done_views:return
        self.done_views.add(token)
        item=self.item(target);kind,body=self.inner(item)
        if kind not in NOMINAL:return
        owner=self.path(target);self.exposed_nominals.add(owner); deferred=[]; has_deref_mut=False
        for id in body.get('impls',[]):
            impl=Ref(target.doc,str(id)); ik,ib=self.inner(self.item(impl));require(ik=='impl','non-impl in impl list')
            trait=ib.get('trait');trait_ref=Ref(impl.doc,str(trait['id'])) if trait else None
            if trait_ref and not self.trait_visible(trait_ref):continue
            context=self.norm({k:ib[k] for k in ['generics','for','trait','is_unsafe','is_negative','is_synthetic','blanket_impl'] if k in ib},impl.doc)
            trait_name=self.path(trait_ref) if trait_ref else ''
            if trait_ref:
                overload=digest(context)
                self.add('trait_impl',path+'::<'+trait_name+'>',context,owner,overload=overload)
                has_deref_mut |= trait_name.endswith('::DerefMut')
            else: overload=''
            for member in ib['items']:
                mr=Ref(impl.doc,str(member));mi=self.item(mr);mk,mb=self.inner(mi)
                if not trait_ref and mi['visibility']!='public':
                    if mk=='function':
                        spec=self.common(mi)|{'definition':self.norm({k:v for k,v in mb.items() if k!='has_body'},impl.doc,False),'impl':context}
                        key=owner+'::'+mi['name']; entry={'owner':owner,'method':mi['name'],'signature_sha256':digest(spec),'previous_visibility':mi['visibility']}
                        self.private_methods.setdefault(key,[])
                        if entry not in self.private_methods[key]:self.private_methods[key].append(entry)
                    continue
                if trait_ref and mi['visibility'] not in ('public','default'):continue
                if trait_name.endswith('::Deref') and mk=='assoc_type' and mi['name']=='Target': deferred.append(mb.get('type'))
                if deref:
                    if mk!='function':continue
                    args=mb['sig']['inputs']
                    if not args or args[0][0]!='self':continue
                    receiver=args[0][1].get('borrowed_ref')
                    if receiver is None:continue
                    if receiver['is_mutable'] and not mutable:continue
                label=('deref_' if deref else '')+('trait_method' if trait_ref else 'inherent_method')
                mp=path+('::<'+trait_name+'>' if trait_ref else '')+'::'+mi['name']
                self.member(mr,mp,label,owner,context,overload)
            for name in ib.get('provided_trait_methods',[]):
                if deref: continue
                # The exact default-provider identity is stable only with equal external tool/dependency inputs.
                self.add('provided_trait_method',path+'::<'+trait_name+'>::'+name,{'provider':trait_name,'method':name,'impl':context},owner,name,digest(context))
        for ty in deferred:
            if not ty or 'resolved_path' not in ty:
                self.opaque.add('non-nominal-deref:'+owner);continue
            other=self.resolve(Ref(target.doc,str(ty['resolved_path']['id'])),False)
            if other:self.methods(other,path+'::<deref>',True,mutable and has_deref_mut,ancestry+(target,))
            else:self.opaque.add('external-deref:'+owner)

    def owned_trait_impls(self):
        # A dependency definition does not contain impls added by its downstream core.
        # Include locally owned public-trait impl declarations on exposed nominal,
        # builtin or external targets, and conditional blanket impls conservatively.
        added=False
        for n,doc in enumerate(self.docs):
            if self.names[n] not in {self.names[0],'neovm_value'}:continue
            for id,item in doc['index'].items():
                ref=Ref(n,id)
                if ref in self.done_impls or item['crate_id']!=doc['index'][str(doc['root'])]['crate_id']:continue
                kind,body=self.inner(item)
                if kind!='impl' or not body.get('trait'):continue
                ty=body['for'];normalized=self.norm(ty,n,False)
                nominal=ty.get('resolved_path')
                path=self.path(Ref(n,str(nominal['id']))) if nominal else None
                exposed=path in self.exposed_nominals or digest(normalized) in self.exposed_forms or 'generic' in ty
                if not exposed:continue
                trait_ref=Ref(n,str(body['trait']['id']))
                if not self.trait_visible(trait_ref):continue
                self.done_impls.add(ref);added=True
                context=self.norm({k:body[k] for k in ['generics','for','trait','is_unsafe','is_negative','is_synthetic','blanket_impl'] if k in body},n)
                owner=path or '@type::'+digest(normalized);trait_path=self.path(trait_ref)
                label=owner+'::<'+trait_path+'>';overload=digest(context)
                self.add('owned_trait_impl',label,context,owner,overload=overload)
                for id in body['items']:
                    mr=Ref(n,str(id));mi=self.item(mr)
                    if mi['visibility'] in ('public','default'):
                        self.member(mr,label+'::'+mi['name'],'owned_trait_member',owner,context,overload)
                for name in body.get('provided_trait_methods',[]):
                    self.add('owned_provided_trait_method',label+'::'+name,{'provider':trait_path,'method':name,'impl':context},owner,name,overload)
        return added

    def extract(self):
        root=Ref(0,str(self.docs[0]['root']));self.exported_walk(root,self.names[0])
        # Public signature edges can expose otherwise unnamed return/argument types.
        done=set()
        visible_nominals={self.alias_target(ref) for ref,path in self.type_views}
        while True:
            pending=[ref for ref in self.sigrefs if ref not in done]
            if not pending:break
            for ref in pending:
                done.add(ref);kind,_=self.inner(self.item(ref))
                if ref in visible_nominals:continue
                if kind=='trait':self.public_traits.add(ref)
                self.describe(ref,'@signature::'+self.path(ref))
                if kind in NOMINAL | {'type_alias'}:self.type_views.append((ref,'@signature::'+self.path(ref)))
        for ref,path in self.type_views:self.methods(ref,path)
        # Methods may reveal new types. Drain to a fixed point without public-name guessing.
        while True:
            more_impls=self.owned_trait_impls()
            if not self.sigrefs-done and not more_impls:break
            for ref in list(self.sigrefs-done):
                done.add(ref);kind,_=self.inner(self.item(ref))
                if kind=='trait':
                    self.public_traits.add(ref);self.done_views.clear()
                p='@signature::'+self.path(ref);self.describe(ref,p)
                if kind in NOMINAL | {'type_alias'}:
                    self.type_views.append((ref,p));self.methods(ref,p)
            for ref,path in self.type_views:self.methods(ref,path)
        return {'status':'EXTRACTED_NOT_A_CANDIDATE_APPROVAL','format_version':self.docs[0]['format_version'],
                'crate':self.names[0],'inputs':self.inputs,'rows':self.rows,'private_inherent_method_catalogue':self.private_methods,'opaque_dependency_crates':sorted(self.opaque_crates),'builtin_alias_signatures':self.builtin_aliases,
                'opaque_external_or_non_nominal_targets':sorted(self.opaque),
                'external_precondition':'Same compiler,target,Cargo.lock and resolved external dependency identities required; external default methods identify exact provider/name. Moved local/value definitions may not be omitted.'}

def load_maps(path):
    if not path:return []
    with open(path) as f:
        reader=csv.DictReader(f,delimiter='\t');require(set(reader.fieldnames or [])=={'from','to'},'path map needs exactly from,to columns');rows=list(reader)
    require(all(set(r)=={'from','to'} for r in rows),'path map row schema mismatch')
    pairs=[(r['from'],r['to']) for r in rows];require(all(a and b for a,b in pairs),'empty path mapping')
    require(len({a for a,b in pairs})==len(pairs),'duplicate path mapping')
    return pairs

EXCEPTION_COLUMNS={'owner','method','candidate_signature_sha256','guard','protected_caller','approval'}
def exceptions(path):
    with open(path) as f:
        reader=csv.DictReader(f,delimiter='\t');require(set(reader.fieldnames or [])==EXCEPTION_COLUMNS,'exception TSV exact columns required');rows=list(reader)
    result={}
    for row in rows:
        require(all(row.values()),'blank exception field')
        require(re.fullmatch(r'[0-9a-f]{64}',row['candidate_signature_sha256']) is not None,'exception signature must be exact SHA256')
        key=(row['owner'],row['method']);require(key not in result,'duplicate exception method '+str(key));result[key]=row
    return result

def valid_identity(row):
    return isinstance(row,dict) and set(row)=={'baseline_sha256','candidate_sha256'} and isinstance(row.get('baseline_sha256'),str) and re.fullmatch(r'[0-9a-f]{64}',row['baseline_sha256']) is not None and row['baseline_sha256']==row.get('candidate_sha256')

def compare(a,b,accepted,external_invariants=None):
    old=a['rows'];new=b['rows'];added=[new[k] for k in sorted(new.keys()-old.keys())];removed=[old[k] for k in sorted(old.keys()-new.keys())]
    changed=[{'key':k,'before':old[k],'after':new[k]} for k in sorted(new.keys()&old.keys()) if old[k]!=new[k]]
    groups=defaultdict(set);unaccepted=[]
    for row in added:
        if row['kind'] in METHOD_KINDS:groups[(row['owner'],row['method'])].add(row['signature_sha256'])
        else:unaccepted.append(row)
    exact=True;errors=[]
    if removed or changed or unaccepted:exact=False;errors.append('non-exception API change')
    if set(groups)!=set(accepted):exact=False;errors.append('actual method groups != exact exception table')
    for key,hashes in groups.items():
        if key not in accepted or hashes!={accepted[key]['candidate_signature_sha256']}:
            exact=False;errors.append('candidate signature mismatch for '+str(key))
    for key in groups:
        expected={r['signature_sha256'] for r in a.get('private_inherent_method_catalogue',{}).get(key[0]+'::'+key[1],[])}
        if not expected or groups[key] != expected:
            exact=False;errors.append('added method must retain exact previously-private baseline signature '+str(key))
    required=set(a.get('opaque_dependency_crates',[]))|set(b.get('opaque_dependency_crates',[]))
    invariants=external_invariants or {}
    dependencies=invariants.get('dependencies',{})
    if required:
        if set(invariants)!={'toolchain','target','cargo_lock','features','dependencies'}:
            exact=False;errors.append('paired toolchain/target/Cargo.lock/features/dependency source evidence required')
        for label in ('toolchain','target','cargo_lock','features'):
            row=invariants.get(label,{})
            if not valid_identity(row):exact=False;errors.append('paired invariant absent/different: '+label)
    elif invariants:
        exact=False;errors.append('unexpected opaque dependency evidence with no opaque dependencies')
    if required != set(dependencies):
        exact=False;errors.append('exact opaque dependency identity evidence required: '+','.join(sorted(required)))
    for crate,row in dependencies.items():
        if not valid_identity(row):
            exact=False;errors.append('opaque dependency source identity differs or invalid: '+crate)
    if a['opaque_external_or_non_nominal_targets']!=b['opaque_external_or_non_nominal_targets']:
        exact=False;errors.append('external/non-nominal closure changed')
    return {'status':'EXACT_EXCEPTION_SET' if exact else 'RED_PUBLIC_API_DELTA','passed':exact,'errors':errors,
            'added':added,'removed':removed,'changed':changed,'actual_added_inherent_method_groups':[{'owner':k[0],'method':k[1],'signature_sha256':sorted(v)} for k,v in sorted(groups.items())],
            'exception_rows':list(accepted.values()),'opaque_dependency_identity_evidence':invariants}

def main():
    p=argparse.ArgumentParser();p.add_argument('--baseline',required=True);p.add_argument('--candidate');p.add_argument('--baseline-receipt',required=True);p.add_argument('--candidate-receipt');p.add_argument('--baseline-dep-receipt',action='append',default=[]);p.add_argument('--candidate-dep-receipt',action='append',default=[]);p.add_argument('--baseline-dep',action='append',default=[]);p.add_argument('--candidate-dep',action='append',default=[]);p.add_argument('--path-map');p.add_argument('--exceptions');p.add_argument('--external-invariants');p.add_argument('--out',required=True)
    args=p.parse_args();out=Path(args.out);require(not out.exists(),'preserve evidence: output directory already exists');out.mkdir(parents=True)
    try:
        ag=Graph([args.baseline]+args.baseline_dep,load_maps(args.path_map));ap=producer_receipts([args.baseline_receipt]+args.baseline_dep_receipt,ag.inputs);a=ag.extract();a['producer_receipts']=ap;(out/'baseline-surface.json').write_text(json.dumps(a,sort_keys=True,separators=(',',':'))+'\n')
        if not args.candidate:
            print(json.dumps({'status':'BASELINE_EXTRACTED_CANDIDATE_PENDING','rows':len(a['rows']),'opaque_targets':len(a['opaque_external_or_non_nominal_targets'])}));return 0
        require(args.exceptions,'compare requires explicit exact exception TSV, including empty header-only table')
        require(args.candidate_receipt,'candidate producer receipt required');bg=Graph([args.candidate]+args.candidate_dep,load_maps(args.path_map));bp=producer_receipts([args.candidate_receipt]+args.candidate_dep_receipt,bg.inputs);require(ap[0]['record']['tools']==bp[0]['record']['tools'],'core compiler/cargo/rustdoc toolchains differ');require(ag.docs[0]['target']==bg.docs[0]['target'],'core target configuration differs');b=bg.extract();b['producer_receipts']=bp;(out/'candidate-surface.json').write_text(json.dumps(b,sort_keys=True,separators=(',',':'))+'\n')
        result=compare(a,b,exceptions(args.exceptions),json.loads(Path(args.external_invariants).read_text()) if args.external_invariants else None);result['inputs']={'external_invariants_file':({'path':str(Path(args.external_invariants).resolve()),'sha256':hashlib.sha256(Path(args.external_invariants).read_bytes()).hexdigest()} if args.external_invariants else None),'baseline':a['inputs'],'candidate':b['inputs'],'baseline_producers':ap,'candidate_producers':bp,'exception_tsv':{'path':str(Path(args.exceptions).resolve()),'sha256':hashlib.sha256(Path(args.exceptions).read_bytes()).hexdigest()},'canonical_path_map':load_maps(args.path_map)};(out/'comparison.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'status':result['status'],'before_rows':len(a['rows']),'after_rows':len(b['rows']),'added':len(result['added']),'removed':len(result['removed']),'changed':len(result['changed'])}));return 0 if result['passed'] else 1
    except (Refusal,KeyError,TypeError,ValueError) as error:
        failure={'status':'REFUSED_UNSUPPORTED_OR_UNRESOLVED','reason':str(error)};(out/'REFUSAL.json').write_text(json.dumps(failure,indent=2)+'\n');print(json.dumps(failure),file=sys.stderr);return 2

if __name__=='__main__':
    try:sys.exit(main())
    except Refusal as e:print('REFUSED: '+str(e),file=sys.stderr);sys.exit(2)
