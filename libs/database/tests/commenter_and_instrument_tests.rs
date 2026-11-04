//! Tests for Sqlcommenter and `DbOperation` constants.

use omni_database::{DbOperation, append_sqlcomment, format_sqlcomment};

#[test]
fn test_sqlcommenter_format() {
    let comment = format_sqlcomment(
        "order-service",
        Some("4bf92f3577b34da6a3ce929d0e0e4736"),
        Some("00f067aa0ba902b7"),
    );

    assert_eq!(
        comment,
        "/*application='order-service',traceparent='00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01'*/"
    );

    let comment_trace_only = format_sqlcomment(
        "order-service",
        Some("4bf92f3577b34da6a3ce929d0e0e4736"),
        None,
    );
    assert_eq!(
        comment_trace_only,
        "/*application='order-service',trace_id='4bf92f3577b34da6a3ce929d0e0e4736'*/"
    );
}

#[test]
fn test_append_sqlcomment() {
    let sql_with_semi = "SELECT * FROM orders WHERE id = $1;";
    let comment = "/*app='test'*/";
    let appended = append_sqlcomment(sql_with_semi, comment);
    assert_eq!(
        appended,
        "SELECT * FROM orders WHERE id = $1 /*app='test'*/;"
    );

    let sql_without_semi = "SELECT * FROM orders WHERE id = $1";
    let appended2 = append_sqlcomment(sql_without_semi, comment);
    assert_eq!(
        appended2,
        "SELECT * FROM orders WHERE id = $1 /*app='test'*/"
    );
}

#[test]
fn test_db_operations() {
    let sel = DbOperation::select("users");
    assert_eq!(sel.op, "SELECT");
    assert_eq!(sel.collection, "users");

    let ins = DbOperation::insert("orders");
    assert_eq!(ins.op, "INSERT");
    assert_eq!(ins.collection, "orders");

    let upd = DbOperation::update("products");
    assert_eq!(upd.op, "UPDATE");
    assert_eq!(upd.collection, "products");

    let del = DbOperation::delete("tokens");
    assert_eq!(del.op, "DELETE");
    assert_eq!(del.collection, "tokens");
}
