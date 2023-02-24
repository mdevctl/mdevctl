#!/bin/sh
# A basic utility script used for debugging versioning of call-out scripts
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

print_supported_version() {
    case "$1" in
        11111111-1111-0000-0000-000000000000)
            # full version 2 and RC=0
            echo "{\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 0
        ;;
        11111111-1111-0000-0000-111111111111)
            # valid json with RC=1
            echo "{\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 1
        ;;
        11111111-1111-0000-0000-222222222222)
            # valid json with RC=2
            echo "{\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 2
        ;;
        11111111-1111-0000-0000-aaaaaaaaaaaa)
            # no json output at all
            echo "This output is bad"
            exit 0
        ;;
        11111111-1111-0000-0000-bbbbbbbbbbbb)
            # extra action dummy
            echo "{\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\",\"dummy\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 0
        ;;
        11111111-1111-0000-0000-cccccccccccc)
            # extra event dummy
            echo "{\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\",\"dummy\"]"
            echo "}}"
            exit 0
        ;;
        11111111-1111-0000-0000-dddddddddddd)
            # action modify missing
            echo "{\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 0
        ;;
        11111111-1111-0000-0000-eeeeeeeeeeee)
            # extra valid data contained
            echo "{\"provides\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "},\"supports\":{"
            echo "\"version\":2,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 0
        ;;
        11111111-1111-0000-0000-ffffffffffff)
            # invalid json
            echo "{\"supports\":{{{"
            echo "\"version\":111111,"
            echo "\"actors\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"inventors\":[\"pre\",\"post\",\"notify\",\"get\"]"
            echo "}}"
            exit 0
        ;;
        *)
            exit 1
        ;;
    esac
}

case "$event" in
    get)
        case "$action" in
            capabilities)
                print_supported_version $uuid
            ;;
            attributes)
                echo "[{\"attribute0\": \"VALUE\"}]"
                exit 0
            ;;
            *)
                exit 1
            ;;
        esac
    ;;
    pre)
        case "$action" in
            define)
                exit 0
            ;;
            undefine)
                exit 0
            ;;
            start)
                exit 0
            ;;
            stop)
                exit 0
            ;;
            modify)
                exit 0
            ;;
            *)
                exit 1
            ;;
        esac
    ;;
    post)
        case "$action" in
            define)
                exit 0
            ;;
            undefine)
                exit 0
            ;;
            start)
                exit 0
            ;;
            stop)
                exit 0
            ;;
            modify)
                exit 0
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
