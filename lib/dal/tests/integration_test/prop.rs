use dal::{
    test_harness::{create_prop, create_prop_of_kind, create_schema, create_schema_variant},
    DalContext, HistoryActor, Prop, PropKind, SchemaKind, StandardModel, Visibility, WriteTenancy,
};
use pretty_assertions_sorted::assert_eq;
use std::option::Option::None;

use crate::dal::test;

#[test]
async fn new(ctx: &DalContext) {
    let _write_tenancy = WriteTenancy::new_universal();
    let _visibility = Visibility::new_head(false);
    let _history_actor = HistoryActor::SystemInit;
    let prop = Prop::new(ctx, "coolness", PropKind::String, None)
        .await
        .expect("cannot create prop");
    assert_eq!(prop.name(), "coolness");
    assert_eq!(prop.kind(), &PropKind::String);
}

#[test]
async fn schema_variants(ctx: &DalContext) {
    let schema = create_schema(ctx, &SchemaKind::Configuration).await;
    let schema_variant = create_schema_variant(ctx, *schema.id()).await;
    let prop = create_prop(ctx).await;

    prop.add_schema_variant(ctx, schema_variant.id())
        .await
        .expect("cannot add schema variant");

    let relations = prop
        .schema_variants(ctx)
        .await
        .expect("cannot get schema variants");
    assert_eq!(relations, vec![schema_variant.clone()]);

    prop.remove_schema_variant(ctx, schema_variant.id())
        .await
        .expect("cannot remove schema variant");

    let relations = prop
        .schema_variants(ctx)
        .await
        .expect("cannot get schema variants");
    assert_eq!(relations, vec![]);
}

#[test]
async fn parent_props(ctx: &DalContext) {
    let parent_prop = create_prop_of_kind(ctx, PropKind::Object).await;
    let child_prop = create_prop_of_kind(ctx, PropKind::String).await;
    child_prop
        .set_parent_prop(ctx, *parent_prop.id())
        .await
        .expect("cannot set parent prop");
    let retrieved_parent_prop = child_prop
        .parent_prop(ctx)
        .await
        .expect("cannot get parent prop")
        .expect("there was no parent prop and we expected one!");
    assert_eq!(retrieved_parent_prop, parent_prop);

    let children = parent_prop
        .child_props(ctx)
        .await
        .expect("should have children");
    assert_eq!(children, vec![child_prop]);
}

#[test]
async fn parent_props_wrong_prop_kinds(ctx: &DalContext) {
    let parent_prop = create_prop_of_kind(ctx, PropKind::String).await;
    let child_prop = create_prop_of_kind(ctx, PropKind::Object).await;

    let result = child_prop.set_parent_prop(ctx, *parent_prop.id()).await;
    result.expect_err("should have errored, and it did not");
}
