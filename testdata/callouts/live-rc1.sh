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
        11111111-1111-1111-0000-000000000000)
            # valid version 3 and RC=0
            echo "{\"supports\":{"
            echo "\"version\":3,"
            echo "\"actions\":[\"start\",\"stop\",\"define\",\"undefine\",\"modify\",\"attributes\",\"capabilities\"],"
            echo "\"events\":[\"pre\",\"post\",\"notify\",\"get\",\"live\"]"
            echo "}}"
            exit 0
        ;;
        *)
            exit 2
        ;;
    esac
}

case "$event" in
    get)
        case "$action" in
            capabilities)
                print_supported_version $uuid
            ;;
            *)
                exit 0
            ;;
        esac
    ;;
    live)
        exit 1
    ;;
    *)
        exit 0
    ;;
esac
