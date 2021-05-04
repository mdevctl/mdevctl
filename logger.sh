#!/usr/bin/bash
# A basic utility script used for debugging notification call-out events
# in mdevctl. Output can be observed via system logs.

#stdin | -e event -a action -s state -u uuid -p parent
shift
event=$1
shift 2
action=$1
shift 2
state=$1
shift 2
uuid=$1
shift 2
parent=$1
json=$(</dev/stdin)

if [[ "$event" == "notify" ]]; then
	echo "Event: $event, Action: $action, State: $state, UUID: $uuid, Parent: $parent, JSON:" | logger -t "mdevctl_logger"
	echo "$json" | logger -t "mdevctl_logger"
fi
