use super::*;

fn parse_and_find(source: &str) -> Vec<CodeConnection> {
    let tree = gd_core::parser::parse(source).unwrap();
    find_connect_calls(tree.root_node(), source)
}

#[test]
fn simple_self_signal() {
    let conns = parse_and_find("extends Node\nfunc _ready():\n\tmy_signal.connect(_on_signal)\n");
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].receiver, "self");
    assert_eq!(conns[0].signal, "my_signal");
    assert_eq!(conns[0].handler, "_on_signal");
}

#[test]
fn chained_receiver() {
    let conns = parse_and_find(
        "extends Node\nfunc _ready():\n\tEventBus.config_updated.connect(_handler)\n",
    );
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].receiver, "EventBus");
    assert_eq!(conns[0].signal, "config_updated");
    assert_eq!(conns[0].handler, "_handler");
}

#[test]
fn node_path_receiver() {
    let conns =
        parse_and_find("extends Node\nfunc _ready():\n\t$Player.hit.connect(_on_player_hit)\n");
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].signal, "hit");
    assert_eq!(conns[0].handler, "_on_player_hit");
}

#[test]
fn multiple_connects() {
    let conns = parse_and_find(
        "extends Node\nfunc _ready():\n\ta.connect(ha)\n\tb.connect(hb)\n\tc.d.connect(hc)\n",
    );
    assert_eq!(conns.len(), 3);
    assert_eq!(conns[0].signal, "a");
    assert_eq!(conns[1].signal, "b");
    assert_eq!(conns[2].signal, "d");
}

#[test]
fn no_connects() {
    let conns = parse_and_find("extends Node\nfunc _ready():\n\tpass\n");
    assert!(conns.is_empty());
}

#[test]
fn disconnect_not_matched() {
    let conns =
        parse_and_find("extends Node\nfunc _ready():\n\tmy_signal.disconnect(_on_signal)\n");
    assert!(conns.is_empty());
}
