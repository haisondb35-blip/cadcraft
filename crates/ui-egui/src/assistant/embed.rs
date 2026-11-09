//! Save chat with file: the assistant's conversation about a drawing, written into its DXF file
//! when the person turns it on (off by default), so it travels with the file and comes back when
//! the file is opened on another computer.
//!
//! The conversation (portable form, without this computer's conversation id, see
//! `Memory::to_json`) is kept in comments at the top of the file
//! (`cadcraft_engine::assistant_chat`). DWG and binary DXF files keep it on this computer only.

use serde_json::Value;

/// How much of the top of a file is read to find its conversation (opening a file): the
/// conversation's comments come first.
const MAX_HEAD: u64 = 12 << 20;

/// Files the conversation can be saved in.
pub fn can_embed(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dxf"))
}

/// Has saves put the conversation `a` keeps for each drawing into it, and `a` read it back.
pub fn install(a: &mut super::Assistant) {
    let shelf = a.shelf.clone();
    cadcraft_engine::assistant_chat::set_hook(Box::new(move |path| {
        if !can_embed(path) {
            return None;
        }
        shelf.lock().ok().and_then(|s| s.chat_for(path))
    }));
    a.embedded_reader = Some(read);
}

/// The conversation saved in the file at `path`, if it has one.
pub fn read(path: &str) -> Option<Value> {
    use std::io::Read;
    if !can_embed(path) {
        return None;
    }
    let mut head = Vec::new();
    std::fs::File::open(path).ok()?.take(MAX_HEAD).read_to_end(&mut head).ok()?;
    cadcraft_engine::assistant_chat::chat_in(&head)
}
