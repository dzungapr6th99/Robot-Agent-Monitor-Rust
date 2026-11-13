use crate::Resource;

pub struct DbTableAttribute {
    pub name: &'static str,
    /// Pairs of API field name and physical database column.
    pub fields: &'static [(&'static str, &'static str)],
    pub child: Option<(&'static str, Resource, &'static str)>,
    pub keys: &'static [&'static str],
}

impl DbTableAttribute {
    pub fn column_for(&self, field: &str) -> Option<&'static str> {
        self.fields
            .iter()
            .find_map(|&(name, column)| (name == field).then_some(column))
    }
}
