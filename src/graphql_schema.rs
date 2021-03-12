use juniper::{
    graphql_object, EmptySubscription, FieldResult, GraphQLEnum, 
    GraphQLInputObject, GraphQLObject, ScalarValue, EmptyMutation, RootNode
};

#[derive(GraphQLObject)]
#[graphql(description = "A resource that we manage")]
struct Resource {
    id: i32,
    name: String,
}

pub struct Context {}

impl juniper::Context for Context{
}

pub struct Query;

#[graphql_object(context=Context)]
impl Query {
   fn resources() -> Vec<Resource> {
        vec![
            Resource {
                id: 1,
                name: "Uwe".into()
            }
        ]
    }
}

pub type Schema = RootNode<'static, Query, EmptyMutation<Context>, EmptySubscription<Context>>;

