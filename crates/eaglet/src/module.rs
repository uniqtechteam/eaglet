//! Modules and how eaglet checks and orders them at startup.

use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::{Container, Dep, Injectable, ProviderDef, StartupError};

/// A module: what it imports, provides and exports. Implemented by `#[module]`.
pub trait Module: 'static {
    /// The module's definition.
    fn definition() -> ModuleDef;
}

/// What `#[module(...)]` declares.
#[derive(Clone, Debug)]
pub struct ModuleDef {
    /// Identity of the module struct.
    pub id: TypeId,
    /// Its name, for messages.
    pub name: &'static str,
    /// Modules whose exports this one can use.
    pub imports: Vec<fn() -> ModuleDef>,
    /// Providers declared here.
    pub providers: Vec<ProviderDef>,
    /// Providers that importing modules may use.
    pub exports: Vec<Dep>,
    /// Controllers whose routes this module serves.
    pub controllers: Vec<ControllerDef>,
}

/// An `#[injectable]` struct with routes. Implemented by `#[controller]` on its `impl` block.
pub trait Controller: Injectable {
    /// The routes, with handlers bound to the built controller.
    ///
    /// # Errors
    /// When the controller or something its routes need was not built.
    fn router(container: &Container) -> Result<axum::Router, StartupError>;

    /// Providers the routes need besides the controller's own fields (for example guards).
    #[must_use]
    fn route_deps() -> Vec<Dep> {
        Vec::new()
    }
}

/// Builds one controller's routes from the built providers.
pub type RouterFn = fn(&Container) -> Result<axum::Router, StartupError>;

/// A controller as declared in `#[module(controllers = [...])]`.
#[derive(Clone)]
pub struct ControllerDef {
    /// The controller itself, built like any provider.
    pub provider: ProviderDef,
    /// What its routes need besides its fields.
    pub route_deps: Vec<Dep>,
    /// Builds its routes.
    pub router: RouterFn,
}

impl ControllerDef {
    /// The definition of controller `C`.
    #[must_use]
    pub fn of<C: Controller>() -> Self {
        Self {
            provider: ProviderDef::of::<C>(),
            route_deps: C::route_deps(),
            router: C::router,
        }
    }
}

impl std::fmt::Debug for ControllerDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControllerDef")
            .field("controller", &self.provider.key.short_name())
            .finish_non_exhaustive()
    }
}

/// The checked module tree: providers in build order and the routers to build afterwards.
pub(crate) struct Resolved {
    pub providers: Vec<ProviderDef>,
    pub routers: Vec<RouterFn>,
}

/// Every module reachable from the root, root first, each once.
fn collect(root: fn() -> ModuleDef) -> Vec<ModuleDef> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(next) = stack.pop() {
        let def = next();
        if !seen.insert(def.id) {
            continue;
        }
        stack.extend(def.imports.iter().rev().copied());
        out.push(def);
    }
    out
}

struct Graph {
    modules: Vec<ModuleDef>,
    by_id: HashMap<TypeId, usize>,
}

impl Graph {
    /// Providers visible inside module `i`: its own plus the exports of what it imports.
    fn visible(
        &self,
        i: usize,
        memo: &mut HashMap<usize, HashSet<TypeId>>,
        busy: &mut HashSet<usize>,
    ) -> Result<HashSet<TypeId>, StartupError> {
        if let Some(v) = memo.get(&i) {
            return Ok(v.clone());
        }
        if !busy.insert(i) {
            return Ok(HashSet::new()); // modules importing each other: no extra visibility through the loop
        }
        let module = &self.modules[i];
        let mut seen: HashSet<TypeId> = module.providers.iter().map(|p| p.key.id).collect();
        for import in &module.imports {
            let Some(&j) = self.by_id.get(&import().id) else {
                continue;
            };
            let inner = self.visible(j, memo, busy)?;
            for export in &self.modules[j].exports {
                if !inner.contains(&export.id) {
                    return Err(StartupError::BadExport {
                        module: self.modules[j].name.to_owned(),
                        export: export.short_name(),
                    });
                }
                seen.insert(export.id);
            }
        }
        busy.remove(&i);
        memo.insert(i, seen.clone());
        Ok(seen)
    }
}

/// Check the module tree under `root` and return every provider in build order. `overrides` replace
/// providers by type (used by tests) after the checks.
pub(crate) fn resolve(
    root: fn() -> ModuleDef,
    overrides: &HashMap<TypeId, ProviderDef>,
) -> Result<Resolved, StartupError> {
    let modules = collect(root);
    let by_id = modules.iter().enumerate().map(|(i, m)| (m.id, i)).collect();
    let graph = Graph { modules, by_id };

    // each provider type is declared once
    let mut owner: HashMap<TypeId, &'static str> = HashMap::new();
    let mut all: Vec<ProviderDef> = Vec::new();
    for module in &graph.modules {
        for p in &module.providers {
            if let Some(first) = owner.insert(p.key.id, module.name) {
                return Err(StartupError::Duplicate {
                    provider: p.key.short_name(),
                    first: first.to_owned(),
                    second: module.name.to_owned(),
                });
            }
            all.push(p.clone());
        }
        for c in &module.controllers {
            let p = &c.provider;
            if let Some(first) = owner.insert(p.key.id, module.name) {
                return Err(StartupError::Duplicate {
                    provider: p.key.short_name(),
                    first: first.to_owned(),
                    second: module.name.to_owned(),
                });
            }
            all.push(p.clone());
        }
    }

    // every dependency is visible where the provider is declared
    let mut memo = HashMap::new();
    for (i, module) in graph.modules.iter().enumerate() {
        let visible = graph.visible(i, &mut memo, &mut HashSet::new())?;
        let controllers = module
            .controllers
            .iter()
            .map(|c| (&c.provider, c.route_deps.as_slice()));
        let providers = module.providers.iter().map(|p| (p, &[][..]));
        for (p, extra) in providers.chain(controllers) {
            if let Some(missing) = p
                .deps
                .iter()
                .chain(extra)
                .find(|d| !visible.contains(&d.id))
            {
                return Err(StartupError::MissingProvider {
                    provider: p.key.short_name(),
                    needs: missing.short_name(),
                    module: module.name.to_owned(),
                });
            }
        }
    }
    let routers = graph
        .modules
        .iter()
        .flat_map(|m| m.controllers.iter().map(|c| c.router))
        .collect();

    let all: Vec<ProviderDef> = all
        .into_iter()
        .map(|p| overrides.get(&p.key.id).cloned().unwrap_or(p))
        .collect();
    Ok(Resolved {
        providers: order(&all)?,
        routers,
    })
}

/// Dependencies before dependents; a circle is an error that shows the whole circle.
fn order(all: &[ProviderDef]) -> Result<Vec<ProviderDef>, StartupError> {
    let index: HashMap<TypeId, usize> =
        all.iter().enumerate().map(|(i, p)| (p.key.id, i)).collect();
    let mut state = vec![0u8; all.len()]; // 0 new, 1 in progress, 2 done
    let mut out = Vec::with_capacity(all.len());
    let mut path = Vec::new();
    for i in 0..all.len() {
        visit(i, all, &index, &mut state, &mut path, &mut out)?;
    }
    Ok(out)
}

/// Depth-first visit for [`order`]; `path` holds the providers currently being visited.
fn visit(
    i: usize,
    all: &[ProviderDef],
    index: &HashMap<TypeId, usize>,
    state: &mut [u8],
    path: &mut Vec<usize>,
    out: &mut Vec<ProviderDef>,
) -> Result<(), StartupError> {
    match state[i] {
        2 => return Ok(()),
        1 => {
            let start = path.iter().position(|&p| p == i).unwrap_or(0);
            let mut names: Vec<String> = path[start..]
                .iter()
                .map(|&p| all[p].key.short_name())
                .collect();
            names.push(all[i].key.short_name());
            return Err(StartupError::Cycle { path: names });
        }
        _ => {}
    }
    state[i] = 1;
    path.push(i);
    for dep in &all[i].deps {
        if let Some(&j) = index.get(&dep.id) {
            visit(j, all, index, state, path, out)?;
        }
    }
    path.pop();
    state[i] = 2;
    out.push(all[i].clone());
    Ok(())
}
