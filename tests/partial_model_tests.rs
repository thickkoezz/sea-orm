#![allow(unused_imports, dead_code)]

use entity::{Column, Entity};
use sea_orm::{
    prelude::*, sea_query::Alias, DerivePartialModel, FromQueryResult, JoinType, QuerySelect, Set,
};

use crate::common::TestContext;
use common::bakery_chain::*;

mod common;

mod entity {
    use sea_orm::prelude::*;

    #[derive(Debug, Clone, DeriveEntityModel)]
    #[sea_orm(table_name = "foo_table")]
    pub struct Model {
        #[sea_orm(primary_key)]
        id: i32,
        foo: i32,
        bar: String,
        foo2: bool,
        bar2: f64,
    }

    #[derive(Debug, DeriveRelation, EnumIter)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(FromQueryResult, DerivePartialModel)]
#[sea_orm(entity = "Entity")]
struct SimpleTest {
    foo: i32,
    bar: String,
}

#[derive(FromQueryResult, DerivePartialModel)]
#[sea_orm(entity = "<entity::Model as ModelTrait>::Entity")]
struct EntityNameNotAIdent {
    #[sea_orm(from_col = "foo2")]
    foo: i32,
    #[sea_orm(from_col = "bar2")]
    bar: String,
}

#[derive(FromQueryResult, DerivePartialModel)]
#[sea_orm(entity = "Entity")]
struct FieldFromDiffNameColumnTest {
    #[sea_orm(from_col = "foo2")]
    foo: i32,
    #[sea_orm(from_col = "bar2")]
    bar: String,
}

#[derive(FromQueryResult, DerivePartialModel)]
struct FieldFromExpr {
    #[sea_orm(from_expr = "Column::Bar2.sum()")]
    foo: f64,
    #[sea_orm(from_expr = "Expr::col(Column::Id).equals(Column::Foo)")]
    bar: bool,
}

#[derive(FromQueryResult, DerivePartialModel)]
struct Nest {
    #[sea_orm(nested)]
    foo: SimpleTest,
}

#[derive(FromQueryResult, DerivePartialModel)]
struct NestOption {
    #[sea_orm(nested)]
    foo: Option<SimpleTest>,
}

#[derive(FromQueryResult, DerivePartialModel)]
#[sea_orm(entity = "bakery::Entity")]
struct Bakery {
    id: i32,
    #[sea_orm(from_col = "Name")]
    title: String,
}

#[derive(DerivePartialModel)]
#[sea_orm(entity = "cake::Entity", from_query_result)]
struct Cake {
    id: i32,
    name: String,
    #[sea_orm(nested)]
    bakery: Option<Bakery>,
    #[sea_orm(skip)]
    ignore: Ignore,
}

#[derive(DerivePartialModel)]
#[sea_orm(entity = "bakery::Entity", from_query_result)]
struct BakeryDetails {
    #[sea_orm(nested)]
    basics: Bakery,
    #[sea_orm(from_expr = "bakery::Column::ProfitMargin")]
    profit: f64,
}

#[derive(Default)]
struct Ignore {}

async fn fill_data(ctx: &TestContext, link: bool) {
    bakery::Entity::insert(bakery::ActiveModel {
        id: Set(42),
        name: Set("cool little bakery".to_string()),
        profit_margin: Set(4.1),
    })
    .exec(&ctx.db)
    .await
    .expect("insert succeeds");

    cake::Entity::insert(cake::ActiveModel {
        id: Set(13),
        name: Set("Test Cake".to_owned()),
        price: Set(Decimal::ZERO),
        bakery_id: Set(if link { Some(42) } else { None }),
        gluten_free: Set(true),
        serial: Set(Uuid::new_v4()),
    })
    .exec(&ctx.db)
    .await
    .expect("insert succeeds");
}

#[sea_orm_macros::test]
async fn partial_model_left_join_does_not_exist() {
    let ctx = TestContext::new("partial_model_left_join_does_not_exist").await;
    create_tables(&ctx.db).await.unwrap();

    fill_data(&ctx, false).await;

    let cake: Cake = cake::Entity::find()
        .left_join(bakery::Entity)
        .into_partial_model()
        .one(&ctx.db)
        .await
        .expect("succeeds to get the result")
        .expect("exactly one model in DB");

    assert_eq!(cake.id, 13);
    assert_eq!(cake.name, "Test Cake");
    assert!(cake.bakery.is_none());

    ctx.delete().await;
}

#[sea_orm_macros::test]
async fn partial_model_left_join_exists() {
    let ctx = TestContext::new("partial_model_left_join_exists").await;
    create_tables(&ctx.db).await.unwrap();

    fill_data(&ctx, true).await;

    let cake: Cake = cake::Entity::find()
        .left_join(bakery::Entity)
        .into_partial_model()
        .one(&ctx.db)
        .await
        .expect("succeeds to get the result")
        .expect("exactly one model in DB");

    assert_eq!(cake.id, 13);
    assert_eq!(cake.name, "Test Cake");
    assert!(matches!(cake.bakery, Some(Bakery { id: 42, .. })));
    assert_eq!(cake.bakery.unwrap().title, "cool little bakery");

    ctx.delete().await;
}

#[derive(DerivePartialModel)]
#[sea_orm(entity = "bakery::Entity", alias = "factory", from_query_result)]
struct Factory {
    id: i32,
    #[sea_orm(from_col = "Name")]
    plant: String,
}

#[derive(DerivePartialModel)]
#[sea_orm(entity = "cake::Entity", from_query_result)]
struct CakeFactory {
    id: i32,
    name: String,
    #[sea_orm(nested)]
    bakery: Option<Factory>,
}

#[sea_orm_macros::test]
async fn partial_model_left_join_alias() {
    // SELECT "cake"."id" AS "id", "cake"."name" AS "name", "factory"."id" AS "bakery_id", "factory"."name" AS "bakery_plant" FROM "cake" LEFT JOIN "bakery" AS "factory" ON "cake"."bakery_id" = "factory"."id" LIMIT 1
    let ctx = TestContext::new("partial_model_left_join_alias").await;
    create_tables(&ctx.db).await.unwrap();

    fill_data(&ctx, true).await;

    let cake: CakeFactory = cake::Entity::find()
        .join_as(
            JoinType::LeftJoin,
            cake::Relation::Bakery.def(),
            Alias::new("factory"),
        )
        .into_partial_model()
        .one(&ctx.db)
        .await
        .expect("succeeds to get the result")
        .expect("exactly one model in DB");

    assert_eq!(cake.id, 13);
    assert_eq!(cake.name, "Test Cake");
    assert!(matches!(cake.bakery, Some(Factory { id: 42, .. })));
    assert_eq!(cake.bakery.unwrap().plant, "cool little bakery");

    ctx.delete().await;
}

#[sea_orm_macros::test]
async fn partial_model_flat() {
    let ctx = TestContext::new("partial_model_flat").await;
    create_tables(&ctx.db).await.unwrap();

    fill_data(&ctx, true).await;

    let bakery: Bakery = bakery::Entity::find()
        .into_partial_model()
        .one(&ctx.db)
        .await
        .expect("succeeds to get the result")
        .expect("exactly one model in DB");

    assert_eq!(bakery.id, 42);
    assert_eq!(bakery.title, "cool little bakery");

    ctx.delete().await;
}

#[sea_orm_macros::test]
async fn partial_model_nested() {
    // SELECT "bakery"."id" AS "basics_id", "bakery"."name" AS "basics_title", "bakery"."profit_margin" AS "profit" FROM "bakery" LIMIT 1
    let ctx = TestContext::new("partial_model_nested").await;
    create_tables(&ctx.db).await.unwrap();

    fill_data(&ctx, true).await;

    let bakery: BakeryDetails = bakery::Entity::find()
        .into_partial_model()
        .one(&ctx.db)
        .await
        .expect("succeeds to get the result")
        .expect("exactly one model in DB");

    assert_eq!(bakery.basics.id, 42);
    assert_eq!(bakery.basics.title, "cool little bakery");
    assert_eq!(bakery.profit, 4.1);

    ctx.delete().await;
}

#[derive(Debug, FromQueryResult, DerivePartialModel)]
#[sea_orm(entity = "bakery::Entity")]
struct WrongBakery {
    id: String,
    #[sea_orm(from_col = "Name")]
    title: String,
}

#[derive(Debug, FromQueryResult, DerivePartialModel)]
#[sea_orm(entity = "cake::Entity")]
struct WrongCake {
    id: i32,
    name: String,
    #[sea_orm(nested)]
    bakery: Option<WrongBakery>,
}

#[sea_orm_macros::test]
#[ignore = "This currently does not work, as sqlx does not perform type checking when a column is absent.."]
async fn partial_model_optional_field_but_type_error() {
    let ctx = TestContext::new("partial_model_nested").await;
    create_tables(&ctx.db).await.unwrap();

    fill_data(&ctx, false).await;

    let _: DbErr = cake::Entity::find()
        .left_join(bakery::Entity)
        .into_partial_model::<WrongCake>()
        .one(&ctx.db)
        .await
        .expect_err("should error instead of returning an empty Option");

    ctx.delete().await;
}
