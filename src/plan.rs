use crate::{
    binder::{
        BoundAssignmentAST, BoundBaseTableReferenceAST, BoundDeleteStatementAST,
        BoundExpressionAST, BoundInsertStatementAST, BoundSelectElementAST,
        BoundSelectStatementAST, BoundStatementAST, BoundTableReferenceAST,
        BoundUpdateStatementAST,
    },
    catalog::{Column, DataType, Schema},
    common::PageID,
};

#[derive(Debug, Clone)]
pub enum Plan {
    SeqScan(SeqScanPlan),
    Filter(FilterPlan),
    Project(ProjectPlan),
    Insert(InsertPlan),
    Delete(DeletePlan),
    Update(UpdatePlan),
}
impl Plan {
    pub fn schema(&self) -> &Schema {
        match self {
            Plan::SeqScan(plan) => &plan.schema,
            Plan::Filter(plan) => &plan.schema,
            Plan::Project(plan) => &plan.schema,
            Plan::Insert(plan) => &plan.schema,
            Plan::Delete(plan) => &plan.schema,
            Plan::Update(plan) => &plan.schema,
        }
    }
}
#[derive(Debug, Clone)]
pub struct SeqScanPlan {
    pub first_page_id: PageID,
    pub schema: Schema,
}
#[derive(Debug, Clone)]
pub struct FilterPlan {
    pub condition: BoundExpressionAST,
    pub schema: Schema,
    pub child: Box<Plan>,
}
#[derive(Debug, Clone)]
pub struct ProjectPlan {
    pub select_elements: Vec<BoundSelectElementAST>,
    pub schema: Schema,
    pub child: Box<Plan>,
}
#[derive(Debug, Clone)]
pub struct InsertPlan {
    pub first_page_id: PageID,
    pub values: Vec<BoundExpressionAST>,
    pub schema: Schema,
}
#[derive(Debug, Clone)]
pub struct DeletePlan {
    pub first_page_id: PageID,
    pub schema: Schema,
    pub child: Box<Plan>,
}
#[derive(Debug, Clone)]
pub struct UpdatePlan {
    pub first_page_id: PageID,
    pub assignments: Vec<BoundAssignmentAST>,
    pub schema: Schema,
    pub child: Box<Plan>,
}

pub struct Planner {
    statement: BoundStatementAST,
}
impl Planner {
    pub fn new(statement: BoundStatementAST) -> Self {
        Self { statement }
    }
    pub fn plan(&self) -> Plan {
        match &self.statement {
            BoundStatementAST::Select(select_statement) => {
                self.plan_select_statement(select_statement)
            }
            BoundStatementAST::Insert(insert_statement) => {
                self.plan_insert_statement(insert_statement)
            }
            BoundStatementAST::Delete(delete_statement) => {
                self.plan_delete_statement(delete_statement)
            }
            BoundStatementAST::Update(update_statement) => {
                self.plan_update_statement(update_statement)
            }
        }
    }
    fn plan_select_statement(&self, select_statement: &BoundSelectStatementAST) -> Plan {
        let mut plan = self.plan_table_reference(&select_statement.table_reference);
        if let Some(condition) = &select_statement.condition {
            plan = Plan::Filter(FilterPlan {
                condition: condition.clone(),
                schema: plan.schema().clone(),
                child: Box::new(plan),
            });
        }
        if !select_statement.select_elements.is_empty() {
            plan = Plan::Project(ProjectPlan {
                select_elements: select_statement.select_elements.clone(),
                schema: Schema {
                    columns: select_statement
                        .select_elements
                        .iter()
                        .map(|select_element| Column {
                            name: select_element.name.clone(),
                            data_type: select_element.expression.data_type(),
                        })
                        .collect(),
                },
                child: Box::new(plan),
            });
        }
        plan
    }
    fn plan_table_reference(&self, table_reference: &BoundTableReferenceAST) -> Plan {
        match table_reference {
            BoundTableReferenceAST::Base(table_reference) => {
                self.plan_base_table_reference(table_reference)
            }
        }
    }
    fn plan_base_table_reference(&self, table_reference: &BoundBaseTableReferenceAST) -> Plan {
        Plan::SeqScan(SeqScanPlan {
            first_page_id: table_reference.first_page_id,
            schema: table_reference.schema.clone(),
        })
    }
    fn plan_insert_statement(&self, insert_statement: &BoundInsertStatementAST) -> Plan {
        Plan::Insert(InsertPlan {
            first_page_id: insert_statement.first_page_id,
            values: insert_statement.values.clone(),
            schema: Schema {
                columns: vec![Column {
                    name: "__insert_count".to_owned(),
                    data_type: DataType::Int,
                }],
            },
        })
    }
    fn plan_delete_statement(&self, delete_statement: &BoundDeleteStatementAST) -> Plan {
        let mut plan = self.plan_base_table_reference(&delete_statement.table_reference);
        let first_page_id = match &plan {
            Plan::SeqScan(plan) => plan.first_page_id,
            _ => unreachable!(),
        };
        if let Some(condition) = &delete_statement.condition {
            plan = Plan::Filter(FilterPlan {
                condition: condition.clone(),
                schema: plan.schema().clone(),
                child: Box::new(plan),
            });
        }
        Plan::Delete(DeletePlan {
            first_page_id,
            schema: Schema {
                columns: vec![Column {
                    name: "__delete_count".to_owned(),
                    data_type: DataType::Int,
                }],
            },
            child: Box::new(plan),
        })
    }
    fn plan_update_statement(&self, update_statement: &BoundUpdateStatementAST) -> Plan {
        let mut plan = self.plan_base_table_reference(&update_statement.table_reference);
        let first_page_id = match &plan {
            Plan::SeqScan(plan) => plan.first_page_id,
            _ => unreachable!(),
        };
        if let Some(condition) = &update_statement.condition {
            plan = Plan::Filter(FilterPlan {
                condition: condition.clone(),
                schema: plan.schema().clone(),
                child: Box::new(plan),
            });
        }
        Plan::Update(UpdatePlan {
            first_page_id,
            assignments: update_statement.assignments.clone(),
            schema: Schema {
                columns: vec![Column {
                    name: "__update_count".to_owned(),
                    data_type: DataType::Int,
                }],
            },
            child: Box::new(plan),
        })
    }
}
