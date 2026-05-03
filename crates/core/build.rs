// Force a rebuild whenever the migrations directory changes.
//
// `sqlx::migrate!()` reads the migration files at proc-macro expansion time,
// so without this hint Cargo wouldn't know to rebuild this crate when a new
// migration is added and the resulting binary would still hold the old
// list of migrations.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
