use std::{hash::Hash, marker::PhantomData};

use leptos_reactive::{create_effect, ScopeDisposer};
use smallvec::SmallVec;
use taffy::style::Display;

use crate::{
    app_handle::AppContext,
    context::{EventCx, UpdateCx},
    id::Id,
    view::{ChangeFlags, View},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

enum TabState<V> {
    Diff(Box<Diff<V>>),
    Active(usize),
}

pub struct Tab<V, VF, T>
where
    V: View,
    VF: Fn(T) -> V + 'static,
    T: 'static,
{
    id: Id,
    active: usize,
    children: Vec<Option<(V, ScopeDisposer)>>,
    view_fn: VF,
    phatom: PhantomData<T>,
    cx: AppContext,
}

pub fn tab<IF, I, T, KF, K, VF, V>(
    active_fn: impl Fn() -> usize + 'static,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> Tab<V, VF, T>
where
    IF: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: View + 'static,
    T: 'static,
{
    let cx = AppContext::get_current();
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;

    create_effect(cx.scope, move |prev_hash_run| {
        let items = each_fn();
        let items = items.into_iter().collect::<SmallVec<[_; 128]>>();
        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let diff = if let Some(HashRun(prev_hash_run)) = prev_hash_run {
            let mut cmds = diff(&prev_hash_run, &hashed_items);
            let mut items = items
                .into_iter()
                .map(|i| Some(i))
                .collect::<SmallVec<[Option<_>; 128]>>();
            for added in &mut cmds.added {
                added.view = Some(items[added.at].take().unwrap());
            }
            cmds
        } else {
            let mut diff = Diff::default();
            for (i, item) in each_fn().into_iter().enumerate() {
                diff.added.push(DiffOpAdd {
                    at: i,
                    view: Some(item),
                });
            }
            diff
        };
        id.update_state(TabState::Diff(Box::new(diff)), false);
        HashRun(hashed_items)
    });

    create_effect(cx.scope, move |_| {
        let active = active_fn();
        id.update_state(TabState::Active::<T>(active), false);
    });

    Tab {
        id,
        active: 0,
        children: Vec::new(),
        view_fn,
        phatom: PhantomData::default(),
        cx: child_cx,
    }
}

impl<V: View + 'static, VF, T> View for Tab<V, VF, T>
where
    VF: Fn(T) -> V + 'static,
{
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        let child = self
            .children
            .iter_mut()
            .find(|v| v.as_ref().map(|(v, _)| v.id() == id).unwrap_or(false));
        if let Some(child) = child {
            child.as_mut().map(|(view, _)| view as &mut dyn View)
        } else {
            None
        }
    }

    fn children(&mut self) -> Vec<&mut dyn View> {
        self.children
            .iter_mut()
            .filter_map(|child| child.as_mut())
            .map(|child| &mut child.0 as &mut dyn View)
            .collect()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Tab: {}", self.active).into()
    }

    fn update(
        &mut self,
        cx: &mut UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(state) = state.downcast::<TabState<T>>() {
            match *state {
                TabState::Diff(diff) => {
                    AppContext::save();
                    AppContext::set_current(self.cx);
                    apply_diff(cx.app_state, *diff, &mut self.children, &self.view_fn);
                    AppContext::restore();
                }
                TabState::Active(active) => {
                    self.active = active;
                }
            }
            cx.request_layout(self.id());
            for (child, _) in self.children.iter().flatten() {
                cx.request_layout(child.id());
            }
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let nodes = self
                .children
                .iter_mut()
                .enumerate()
                .filter_map(|(i, child)| {
                    let child_id = child.as_ref()?.0.id();
                    let mut child_view = cx.app_state.view_state(child_id);
                    if i != self.active {
                        // set display to none for non active child
                        child_view.style.display = Display::None.into();
                    } else {
                        child_view.style.display = Display::Flex.into();
                    }
                    let node = child.as_mut()?.0.layout_main(cx);
                    Some(node)
                })
                .collect::<Vec<_>>();
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {
        for child in &mut self.children {
            if let Some((child, _)) = child.as_mut() {
                child.compute_layout_main(cx);
            }
        }
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        if let Some(Some((child, _))) = self.children.get_mut(self.active) {
            child.event_main(cx, id_path, event)
        } else {
            false
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(Some((child, _))) = self.children.get_mut(self.active) {
            child.paint_main(cx);
        }
    }
}
