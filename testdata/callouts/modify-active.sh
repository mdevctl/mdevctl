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

tempfile="tmp_modify-live.json"


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
    pre)
        case "$action" in
            modify)
                exit 0
            ;;
            *)
                exit 1
            ;;
        esac
    ;;
    get)
        case "$action" in
            capabilities)
                print_supported_version "$uuid"
            ;;
            attributes)
                if [ -f "$tempfile" ]; then
                    cat "$tempfile"
                    rm "$tempfile"
                else
                        echo "[]"
                fi
                exit 0
            ;;
            *)
                exit 1
            ;;
        esac
    ;;
    post)
        case "$action" in
            modify)
                exit 0
            ;;
            *)
                exit 1
            ;;
        esac
    ;;
    live)
        case "$action" in
            modify)
                echo "$json" | grep -oP '"attrs":+\K(\[.*\])' > "$tempfile"
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
