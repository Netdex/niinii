/// Settings controlling how `Option<T>` values are edited.
#[derive(Clone, Debug, Default)]
pub struct OptionSettings {
    /// Draw the enable checkbox and the value's editor on one line, with the
    /// editor disabled (showing `T::default()`) while the value is `None`.
    /// Otherwise the editor is drawn indented below the checkbox, and only
    /// while the value is `Some`.
    pub inline: bool,
}
