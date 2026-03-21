use serde::Serialize;
use std::collections::HashSet;
use tree_sitter::Node;

/// Coupling Between Objects (CBO) metrics for a single class.
///
/// CBO = number of distinct external types a class is structurally coupled to.
///
/// A class is coupled to another type when it:
///   - extends it (superclass)
///   - implements it (interface)
///   - references it in a property type annotation
///   - references it in a method parameter type annotation
///   - references it in a method return type annotation
///
/// Only structural (type-level) coupling is measured.  Runtime coupling inside
/// method bodies (e.g. `new Foo()`) is intentionally excluded so the metric
/// stays stable under refactoring of implementation details.
///
/// Primitive TypeScript types (`string`, `number`, `boolean`, `void`, `any`,
/// `never`, `unknown`, `undefined`, `null`, `symbol`, `bigint`) are represented
/// by the `predefined_type` node kind in tree-sitter and therefore never appear
/// as `type_identifier` nodes.  They are excluded naturally without any
/// hard-coded filter list.
///
/// Implementation note: in tree-sitter-typescript, `extends X` places X as an
/// `identifier` node (expression context), while `implements Y` places Y as a
/// `type_identifier` node (type context).  The heritage collector therefore
/// captures both node kinds within the heritage clause.
///
/// Reference: Object-Oriented Metrics in Practice (Lanza & Marinescu, 2006)
#[derive(Debug, Clone, Serialize)]
pub struct ClassCbo {
    /// Class name, or `<anonymous>` for unnamed class expressions.
    pub class_name: String,
    /// Line number where the class starts (1-based).
    pub line: usize,
    /// Sorted list of distinct type names this class is coupled to.
    pub coupled_types: Vec<String>,
    /// CBO score = number of distinct coupled types.
    pub cbo: usize,
}

/// Compute CBO for every class found beneath `root`.
pub fn compute_class_cbo(root: Node, source: &[u8]) -> Vec<ClassCbo> {
    let mut results = Vec::new();
    collect_classes(root, source, &mut results);
    results
}

// ---------------------------------------------------------------------------
// AST traversal
// ---------------------------------------------------------------------------

fn collect_classes(node: Node, source: &[u8], out: &mut Vec<ClassCbo>) {
    let is_class = match node.kind() {
        "class_declaration" => true,
        // Anonymous class expression: `const X = class { … }`.
        // The bare `class` keyword token (which is a leaf child of
        // class_declaration) also has kind "class", so guard against it by
        // requiring a body field.
        "class" => node.child_by_field_name("body").is_some(),
        _ => false,
    };
    if is_class {
        out.push(measure_class(node, source));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_classes(child, source, out);
    }
}

fn measure_class(class_node: Node, source: &[u8]) -> ClassCbo {
    let class_name = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>")
        .to_string();

    let line = class_node.start_position().row + 1;

    let mut coupled: HashSet<String> = HashSet::new();

    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        match child.kind() {
            // Heritage clause covers both extends and implements.
            // Uses a broader collector because `extends X` puts X as `identifier`
            // (expression context), not `type_identifier`.
            "class_heritage" => collect_heritage_types(child, source, &mut coupled),
            "class_body" => collect_body_types(child, source, &mut coupled),
            _ => {}
        }
    }

    // A class is not coupled to itself.
    coupled.remove(&class_name);

    let mut coupled_types: Vec<String> = coupled.into_iter().collect();
    coupled_types.sort();
    let cbo = coupled_types.len();

    ClassCbo {
        class_name,
        line,
        coupled_types,
        cbo,
    }
}

/// Collect all type references from the `class_heritage` subtree.
///
/// In tree-sitter-typescript:
///   - `extends X`   → X is an `identifier` (expression context)
///   - `implements Y` → Y is a `type_identifier` (type context)
///
/// Both node kinds are collected here so that the superclass name is captured
/// from the extends clause in addition to interface names from implements.
fn collect_heritage_types(node: Node, source: &[u8], out: &mut HashSet<String>) {
    collect_identifier_and_type_id_recursive(node, source, out);
}

/// Collect type references from class body members.
///
/// Only structural annotations are visited — method statement blocks are skipped
/// so that runtime references (e.g. `new Dep()`) do not inflate the score.
fn collect_body_types(body: Node, source: &[u8], out: &mut HashSet<String>) {
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        match member.kind() {
            // Property declarations: `name: TypeAnnotation [= initialiser]`
            "public_field_definition" | "property_signature" => {
                if let Some(type_node) = member.child_by_field_name("type") {
                    collect_type_identifiers_recursive(type_node, source, out);
                }
            }
            // Method definitions and signatures: collect params + return type, not body
            "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "constructor_type" => {
                collect_method_signature_types(member, source, out);
            }
            _ => {}
        }
    }
}

/// Collect type identifiers from a method's parameters and return type annotation,
/// deliberately skipping the `statement_block` body.
fn collect_method_signature_types(method: Node, source: &[u8], out: &mut HashSet<String>) {
    let mut cursor = method.walk();
    for child in method.children(&mut cursor) {
        match child.kind() {
            "formal_parameters" => collect_type_identifiers_recursive(child, source, out),
            "type_annotation" => collect_type_identifiers_recursive(child, source, out),
            // Do NOT recurse into statement_block (the method body).
            "statement_block" => {}
            _ => {}
        }
    }
}

/// Recursively walk `node` and insert every `type_identifier` into `out`.
///
/// Used for type annotation subtrees where only type-context nodes appear.
fn collect_type_identifiers_recursive(node: Node, source: &[u8], out: &mut HashSet<String>) {
    if node.kind() == "type_identifier" {
        if let Ok(name) = node.utf8_text(source) {
            out.insert(name.to_string());
        }
        // type_identifier is a leaf; no need to descend further.
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_type_identifiers_recursive(child, source, out);
    }
}

/// Recursively walk `node` and insert every `identifier` or `type_identifier` into `out`.
///
/// Used for the heritage clause where superclass names appear as plain `identifier`
/// nodes (expression context) and interface names appear as `type_identifier` nodes
/// (type context).
fn collect_identifier_and_type_id_recursive(node: Node, source: &[u8], out: &mut HashSet<String>) {
    match node.kind() {
        "type_identifier" | "identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                out.insert(name.to_string());
            }
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_and_type_id_recursive(child, source, out);
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — TDD)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_typescript;

    fn cbo_for(src: &str) -> Vec<ClassCbo> {
        let tree = parse_typescript(src).expect("parse failed");
        compute_class_cbo(tree.root_node(), src.as_bytes())
    }

    fn first(src: &str) -> ClassCbo {
        let mut v = cbo_for(src);
        assert!(!v.is_empty(), "expected at least one class");
        v.remove(0)
    }

    // --- no coupling ---

    #[test]
    fn test_empty_class_has_zero_cbo() {
        let src = "class Empty {}";
        let c = first(src);
        assert_eq!(c.class_name, "Empty");
        assert_eq!(c.cbo, 0);
        assert!(c.coupled_types.is_empty());
    }

    #[test]
    fn test_class_with_only_primitives_has_zero_cbo() {
        // string, number, boolean, void are `predefined_type` in tree-sitter, not
        // `type_identifier`, so they are naturally excluded from the CBO count.
        let src = r#"
class Primitive {
    name: string;
    age: number;
    active: boolean;
    greet(prefix: string): void {}
}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 0);
    }

    // --- extends clause ---

    #[test]
    fn test_class_extending_another_has_cbo_one() {
        let src = r#"
class Animal {}
class Dog extends Animal {}
"#;
        let results = cbo_for(src);
        let dog = results.iter().find(|c| c.class_name == "Dog").unwrap();
        assert_eq!(dog.cbo, 1);
        assert_eq!(dog.coupled_types, vec!["Animal"]);
    }

    // --- implements clause ---

    #[test]
    fn test_class_implementing_one_interface_has_cbo_one() {
        let src = r#"
class Task implements Runnable {}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 1);
        assert_eq!(c.coupled_types, vec!["Runnable"]);
    }

    #[test]
    fn test_class_implementing_two_interfaces_has_cbo_two() {
        let src = r#"
class Worker implements Runnable, Serializable {}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 2);
        assert_eq!(c.coupled_types, vec!["Runnable", "Serializable"]);
    }

    #[test]
    fn test_class_extending_and_implementing_has_correct_cbo() {
        let src = r#"
class Service extends BaseService implements ILogger, ICache {}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 3);
        assert_eq!(
            c.coupled_types,
            vec!["BaseService", "ICache", "ILogger"]
        );
    }

    // --- property type annotations ---

    #[test]
    fn test_property_typed_as_class_counts_as_coupling() {
        let src = r#"
class OrderService {
    private repo: OrderRepository;
    private logger: Logger;
}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 2);
        assert_eq!(c.coupled_types, vec!["Logger", "OrderRepository"]);
    }

    // --- method parameter and return types ---

    #[test]
    fn test_method_param_type_counts_as_coupling() {
        let src = r#"
class Processor {
    process(event: DomainEvent): void {}
}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 1);
        assert_eq!(c.coupled_types, vec!["DomainEvent"]);
    }

    #[test]
    fn test_method_return_type_counts_as_coupling() {
        let src = r#"
class Factory {
    create(): Widget {}
}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 1);
        assert_eq!(c.coupled_types, vec!["Widget"]);
    }

    #[test]
    fn test_method_body_references_do_not_count() {
        // Runtime `new Dep()` inside the body should NOT inflate the CBO score.
        let src = r#"
class Service {
    run(): void {
        const dep = new HiddenDependency();
        dep.execute();
    }
}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 0, "method body references should not be counted");
    }

    // --- generic type parameters ---

    #[test]
    fn test_generic_type_argument_counts_as_coupling() {
        // Promise<Result> — Result is a type_identifier inside type_arguments
        let src = r#"
class AsyncService {
    fetch(): Promise<Result> {}
}
"#;
        let c = first(src);
        assert!(
            c.coupled_types.contains(&"Result".to_string()),
            "expected Result in coupled_types, got {:?}",
            c.coupled_types
        );
    }

    // --- self-reference excluded ---

    #[test]
    fn test_self_reference_is_excluded() {
        // Recursive factory: clone(): MyClass — should not couple to itself
        let src = r#"
class MyClass {
    clone(): MyClass {}
}
"#;
        let c = first(src);
        assert_eq!(c.cbo, 0, "self-reference must not be counted");
    }

    // --- heavy coupling ---

    #[test]
    fn test_heavy_coupling_class() {
        let src = r#"
class OrderController extends BaseController implements IController, ILoggable {
    private repo: OrderRepository;
    private mailer: EmailService;

    create(dto: CreateOrderDto): Order {}
    cancel(id: OrderId): Result {}
}
"#;
        let c = first(src);
        // BaseController, IController, ILoggable, OrderRepository, EmailService,
        // CreateOrderDto, Order, OrderId, Result → 9 distinct types
        assert!(c.cbo >= 9, "expected at least 9 coupled types, got {}", c.cbo);
    }

    // --- duplicate types counted once ---

    #[test]
    fn test_same_type_referenced_multiple_times_counted_once() {
        let src = r#"
class RepoAdapter {
    private primary: UserRepository;
    private fallback: UserRepository;
    find(id: UserId): UserRepository {}
}
"#;
        let c = first(src);
        // UserRepository appears 3× but should count once; UserId counts once
        let repo_count = c
            .coupled_types
            .iter()
            .filter(|t| t.as_str() == "UserRepository")
            .count();
        assert_eq!(repo_count, 1, "UserRepository should appear only once");
        assert_eq!(c.cbo, 2); // UserRepository + UserId
    }

    // --- class name extraction ---

    #[test]
    fn test_class_name_is_extracted() {
        let src = "class PaymentGateway {}";
        let c = first(src);
        assert_eq!(c.class_name, "PaymentGateway");
    }

    // --- line number ---

    #[test]
    fn test_line_number_is_correct() {
        let src = "\n\nclass MyService {}\n";
        let c = first(src);
        assert_eq!(c.line, 3);
    }

    // --- multiple classes measured independently ---

    #[test]
    fn test_multiple_classes_measured_independently() {
        let src = r#"
class Isolated {}

class Coupled extends Isolated implements IFoo {}
"#;
        let results = cbo_for(src);
        let isolated = results.iter().find(|c| c.class_name == "Isolated").unwrap();
        let coupled = results.iter().find(|c| c.class_name == "Coupled").unwrap();

        assert_eq!(isolated.cbo, 0);
        assert_eq!(coupled.cbo, 2);
    }
}
