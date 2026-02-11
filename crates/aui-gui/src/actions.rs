use gpui::actions;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        InsertLineBreak,
        Submit,
        AttachFiles,
        ClearAttachments,
        ExportSession,
        HistoryPrev,
        HistoryNext,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Quit,
    ]
);
