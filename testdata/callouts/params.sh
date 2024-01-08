#!/bin/sh
# A basic utility script used for debugging notification call-out events
# in mdevctl. Output can be observed via system logs.

#stdin | -t type -e event -a action -s state -u uuid -p parent
shift
type=$1
shift 2
event=$1
shift 2
action=$1
shift 2
state=$1
shift 2
uuid=$1
shift 2
parent=$1
json=$(cat)

test_params() {
    test_parent=$1
    test_type=$2
    test_state=$3
    test_uuid="976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9"

    if [ "$parent" != "$test_parent" ]; then
        exit 1
    fi
    if [ "$type" != "$test_type" ]; then
        exit 1
    fi
    if [ "$state" != "$test_state" ]; then
        exit 1
    fi
    if [ "$uuid" != "$test_uuid" ]; then
        exit 1
    fi
    exit 0
}

case "$event" in
    pre)
        case "$action" in
            define)
                test_params "test_parent_define" "test_type_define" "none"
            ;;
            undefine)
                test_params "test_parent_undefine" "test_type_undefine" "none"
            ;;
            start)
                test_params "test_parent_start" "test_type_start" "none"
            ;;
            stop)
                test_params "test_parent_stop" "test_type_stop" "none"
            ;;
            modify)
                test_params "test_parent_modify" "test_type_modify" "none"
            ;;
            *)
                exit 1
            ;;
        esac
    ;;
    post)
    case "$action" in
        define)
            test_params "test_parent_define" "test_type_define" "success"
        ;;
        undefine)
            test_params "test_parent_undefine" "test_type_undefine" "success"
        ;;
        start)
            test_params "test_parent_start" "test_type_start" "success"
        ;;
        stop)
            test_params "test_parent_stop" "test_type_stop" "success"
        ;;
        modify)
            test_params "test_parent_modify" "test_type_modify" "success"
        ;;
        *)
            exit 1
        ;;
    esac
    ;;
    *)
        exit 1
    ;;
esac
