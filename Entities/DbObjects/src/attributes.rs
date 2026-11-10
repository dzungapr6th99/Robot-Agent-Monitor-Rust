use crate::Resource;

pub struct DbTableAttribute {
    pub name: &'static str,
    pub fields: &'static [&'static str],
    pub child: Option<(&'static str, Resource, &'static str)>,
    pub keys: &'static [&'static str],
}
