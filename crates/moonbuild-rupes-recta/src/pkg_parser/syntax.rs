#[derive(Debug)]
pub struct ImportItem {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Debug)]
pub enum ImportKind {
    Regular,
    Test,
    Wbtest,
}

#[derive(Debug)]
pub enum Constant {
    Int(i32),
    String(String),
    Bool(bool),
}

#[derive(Debug)]
pub struct MapElem {
    pub key: String,
    pub value: Expr,
}

#[derive(Debug)]
pub enum Expr {
    Id(String),
    Constant(Constant),
    Map(Vec<MapElem>),
    Array(Vec<Expr>),
}

#[derive(Debug)]
pub enum Argument {
    Labeled(String, Expr),
    Positional(Expr),
}

#[derive(Debug)]
pub enum Statement {
    Import(Vec<ImportItem>, ImportKind),
    Assign(String, Expr),
    Apply(String, Vec<Argument>),
}
