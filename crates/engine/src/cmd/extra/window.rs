use super::{
    CellRef, EngineError, Json, Result, Session, SheetViewMode, VIEWS_NAME, WATCH_NAME, Workbook, bad, bool_param, edit, json, ok, split_sheet,
    stash_get, stash_map, stash_set, str_param, target_sheet, u32_param,
};

/// Switches a sheet's view. Like zoom, the view is saved with the file but is not an edit: it
/// neither records an undo step nor marks a clean workbook as changed.
pub(super) fn set_view(s: &mut Session, p: &Json, mode: SheetViewMode) -> Result<Json> {
    let si = target_sheet(s, p)?;
    let d = s.doc_mut()?;
    let dirty = d.is_dirty();
    let wb = std::sync::Arc::make_mut(&mut d.wb);
    if let Some(sh) = wb.sheet_mut(si) {
        sh.view = mode;
    }
    if !dirty {
        d.saved = d.wb.clone();
    }
    Ok(json!({"view": mode.as_str()}))
}

pub(super) fn ruler(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(s.hide_ruler);
    s.hide_ruler = !on;
    Ok(json!({"on": on}))
}

/// The Watch Window list isn't an edit (Excel doesn't undo it): undo and redo keep the current
/// list when they swap in another snapshot of the workbook. Sheets are matched by their session
/// id, so a rename being undone keeps the list too.
pub(crate) fn keep_watch_list(from: &Workbook, to: &mut std::sync::Arc<Workbook>) {
    let kept = |uid: u64| from.sheets.iter().find(|f| f.uid.get() == uid).map(|f| f.cell_watches.as_slice()).unwrap_or_default();
    if to.sheets.iter().all(|sh| sh.cell_watches == kept(sh.uid.get())) {
        return;
    }
    let wb = std::sync::Arc::make_mut(to);
    for sh in wb.sheets.iter_mut() {
        let keep = kept(sh.uid.get());
        if sh.cell_watches != keep {
            std::sync::Arc::make_mut(sh).cell_watches = keep.to_vec();
        }
    }
}

/// Moves a Watch Window list kept in the hidden name `_sc_watch` (how GridCraft saved it before
/// it wrote Excel's `<cellWatches>`) into the sheets, and drops the name.
pub(crate) fn migrate_watch_name(wb: &mut Workbook) {
    if !wb.names.iter().any(|n| n.name == WATCH_NAME) {
        return;
    }
    let old: Vec<String> =
        stash_get(wb, WATCH_NAME).as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    wb.names.retain(|n| n.name != WATCH_NAME);
    for w in old.iter().take(10_000) {
        let (sheet, body) = split_sheet(w);
        let si = sheet.and_then(|n| wb.sheet_index(&n)).unwrap_or(wb.active_sheet());
        let Some(c) = CellRef::parse(body) else { continue };
        if let Some(sh) = wb.sheet_mut(si)
            && !sh.cell_watches.contains(&c)
        {
            sh.cell_watches.push(c);
        }
    }
}

/// Normal, Page Layout and Page Break Preview belong to the window, not to the edits: Excel doesn't
/// undo a switch between them, and `set_view` is no undo step. So undo and redo keep each sheet's
/// current view when they swap in another snapshot of the workbook, and a view kept this way
/// doesn't make a saved workbook dirty.
pub(crate) fn keep_view_modes(d: &mut crate::DocState, now: &Workbook) {
    // Sheets have no stable id. A name missing from `now` is a rename being undone or redone, which
    // keeps the sheet count and position: the sheet is the one at the same index, under a name the
    // restored snapshot doesn't have.
    let renamed = |i: usize| {
        (now.sheets.len() == d.wb.sheets.len()).then(|| now.sheets.get(i)).flatten().filter(|sh| !d.wb.sheets.iter().any(|s| s.name == sh.name))
    };
    // The selected tabs (grouped sheets) are view state as well: Undo neither groups nor ungroups.
    let current = |i: usize, name: &str| now.sheets.iter().find(|sh| sh.name == name).or_else(|| renamed(i)).map(|sh| (sh.view, sh.tab_selected));
    let changed: Vec<(usize, (SheetViewMode, bool))> =
        d.wb.sheets
            .iter()
            .enumerate()
            .filter_map(|(i, sh)| current(i, &sh.name).filter(|v| *v != (sh.view, sh.tab_selected)).map(|v| (i, v)))
            .collect();
    if changed.is_empty() {
        return;
    }
    let dirty = d.is_dirty();
    let wb = std::sync::Arc::make_mut(&mut d.wb);
    for (i, v) in changed {
        if let Some(sh) = wb.sheet_mut(i) {
            (sh.view, sh.tab_selected) = v;
        }
    }
    if !dirty {
        d.saved = d.wb.clone();
    }
}

pub(super) fn custom_views(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let mut views = stash_map(&d.wb, VIEWS_NAME);
    if let Some(name) = str_param(p, "save") {
        let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
        views[name] = json!({"sheet": d.wb.active_sheet(), "selection": d.selection.a1(), "zoom": sh.zoom, "freeze": sh.freeze});
        let v = views.clone();
        edit(s, |cx| {
            stash_set(&mut cx.wb, VIEWS_NAME, &v);
            Ok(())
        })?;
    } else if let Some(name) = str_param(p, "show") {
        let v = views.get(name).cloned().ok_or_else(|| bad("view.customViews", "no such view"))?;
        if let Some(si) = v["sheet"].as_u64() {
            s.execute("sheet.activate", json!({"sheet": si}))?;
        }
        if let Some(sel) = v["selection"].as_str() {
            s.execute("selection.set", json!({"range": sel}))?;
        }
        if let Some(z) = v["zoom"].as_u64() {
            s.execute("view.zoom", json!({"percent": z}))?;
        }
        if let Some(f) = v.get("freeze") {
            let freeze = match (f[0].as_u64(), f[1].as_u64()) {
                (Some(r), Some(c)) => u32::try_from(r).ok().zip(u32::try_from(c).ok()),
                _ => None,
            };
            edit(s, |cx| {
                let si = cx.sheet_index();
                cx.sheet_mut(si)?.freeze = freeze;
                Ok(())
            })?;
        }
    } else if let Some(name) = str_param(p, "delete") {
        if let Some(o) = views.as_object_mut() {
            o.remove(name);
        }
        let v = views.clone();
        edit(s, |cx| {
            stash_set(&mut cx.wb, VIEWS_NAME, &v);
            Ok(())
        })?;
    }
    Ok(stash_get(&s.doc()?.wb, VIEWS_NAME))
}

pub(super) fn split(s: &mut Session, _: &Json) -> Result<Json> {
    let has = s.doc()?.wb.active().is_some_and(|sh| sh.freeze.is_some());
    if has { s.execute("view.unfreezePanes", json!({})) } else { s.execute("view.freezePanes", json!({})) }
}

pub(super) fn new_window(s: &mut Session, _: &Json) -> Result<Json> {
    let id = s.new_window()?;
    let doc = s.window(id).map(|w| w.doc_uid);
    Ok(json!({"window": id, "doc": doc, "title": s.window_title(id)}))
}

/// The window a window command names: `window` (an id), else `doc` (a workbook id: its most
/// recent window), else the active window.
pub(crate) fn window_param(s: &Session, cmd: &str, p: &Json) -> Result<u64> {
    let t = crate::Target::from_json(p).map_err(|e| bad(cmd, e.to_string()))?;
    s.target_window(t).map_err(|e| bad(cmd, e.to_string()))
}

/// View › Arrange All. The engine has no window geometry: it picks the windows and the layout,
/// and the interface moves its native windows ([`Session::window_list`] order, active first).
pub(super) fn arrange_all(s: &mut Session, p: &Json) -> Result<Json> {
    let arrange = match str_param(p, "arrange").unwrap_or("tiled") {
        a @ ("tiled" | "horizontal" | "vertical" | "cascade") => a,
        other => return Err(bad("view.arrangeAll", format!("`arrange` must be tiled, horizontal, vertical or cascade, not `{other}`"))),
    };
    let active = s.active_window();
    let doc = active.and_then(|w| s.window(w)).map(|w| w.doc_uid);
    let only_active = p.get("activeWorkbook").and_then(Json::as_bool).unwrap_or(false);
    let mut windows: Vec<u64> = s.windows().iter().filter(|w| !w.hidden && (!only_active || Some(w.doc_uid) == doc)).map(|w| w.id).collect();
    if let Some(at) = windows.iter().position(|w| Some(*w) == active)
        && let Some(first) = windows.get_mut(..=at)
    {
        first.rotate_right(1);
    }
    Ok(json!({"arrange": arrange, "windows": windows}))
}

pub(super) fn hide_window(s: &mut Session, p: &Json) -> Result<Json> {
    let id = window_param(s, "view.hideWindow", p)?;
    s.hide_window(id);
    ok()
}

pub(super) fn unhide_window(s: &mut Session, p: &Json) -> Result<Json> {
    let id = if p.get("window").is_some_and(|v| !v.is_null()) || p.get("doc").is_some_and(|v| !v.is_null()) {
        Some(window_param(s, "view.unhideWindow", p)?)
    } else if let Some(i) = u32_param(p, "index") {
        // A workbook's position in the document list.
        let uid = s.documents().get(i as usize).map(|d| d.uid).ok_or_else(|| bad("view.unhideWindow", "no such window"))?;
        s.windows().iter().filter(|w| w.doc_uid == uid && w.hidden).map(|w| w.id).next_back().or_else(|| s.doc_window(uid))
    } else {
        s.last_hidden_window()
    };
    if let Some(id) = id {
        s.unhide_window(id);
    }
    ok()
}

pub(super) fn switch_windows(s: &mut Session, p: &Json) -> Result<Json> {
    if p.get("window").is_some_and(|v| !v.is_null()) || p.get("doc").is_some_and(|v| !v.is_null()) {
        let id = window_param(s, "view.switchWindows", p)?;
        s.unhide_window(id);
    }
    Ok(s.window_list())
}

/// Formulas › Watch Window. Each sheet keeps its watched cells (`<cellWatches>`); the list is
/// every sheet's, in sheet order.
pub(super) fn watch_window(s: &mut Session, p: &Json) -> Result<Json> {
    let parse = |wb: &Workbook, w: &str| -> Result<(usize, CellRef)> {
        let (sheet, body) = split_sheet(w.trim());
        let si = match sheet {
            Some(n) => wb.sheet_index(&n).ok_or_else(|| bad("formulas.watchWindow", format!("no sheet named {n}")))?,
            None => wb.active_sheet(),
        };
        let c = CellRef::parse(body.trim().trim_start_matches('$').replace('$', "").as_str())
            .ok_or_else(|| bad("formulas.watchWindow", format!("not a cell: {w}")))?;
        Ok((si, c))
    };
    let add = str_param(p, "add").map(|a| parse(&s.doc()?.wb, a)).transpose()?;
    let remove = str_param(p, "remove").map(|r| parse(&s.doc()?.wb, r)).transpose()?;
    let d = s.doc_mut()?;
    let legacy = d.wb.names.iter().any(|n| n.name == WATCH_NAME);
    if add.is_some() || remove.is_some() || legacy {
        // The watch list is view state: don't add an undo step.
        let wb = std::sync::Arc::make_mut(&mut d.wb);
        migrate_watch_name(wb);
        if let Some((si, c)) = add
            && let Some(sh) = wb.sheet_mut(si)
            && !sh.cell_watches.contains(&c)
        {
            sh.cell_watches.push(c);
        }
        if let Some((si, c)) = remove
            && let Some(sh) = wb.sheet_mut(si)
        {
            sh.cell_watches.retain(|x| *x != c);
        }
    }
    let d = s.doc()?;
    let rows: Vec<Json> =
        d.wb.sheets
            .iter()
            .flat_map(|sh| sh.cell_watches.iter().map(move |c| (sh, *c)))
            .map(|(sh, c)| {
                json!({
                    "cell": format!("{}!{}", gridcraft_formula::quote_sheet(&sh.name), c.a1()),
                    "sheet": sh.name,
                    "value": sh.value(c).display(),
                    "formula": sh.cell(c).and_then(|x| x.formula.as_ref()).map(|f| format!("={}", f.text)),
                })
            })
            .collect();
    Ok(Json::Array(rows))
}
