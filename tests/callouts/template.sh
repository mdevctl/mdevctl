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

# do stuff here
