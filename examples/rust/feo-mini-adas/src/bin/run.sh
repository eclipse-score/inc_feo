#!/bin/bash

SESSION_NAME="feo"
BIN1="../../../../../target/debug/adas_primary"
BIN2="../../../../../target/debug/adas_secondary_1"
BIN3="../../../../../target/debug/adas_secondary_2"

tmux new-session -d -s $SESSION_NAME -n main
tmux set-option -g mouse on

tmux send-keys "$BIN1"

tmux split-window -v

tmux send-keys "$BIN2"

tmux split-window -v

tmux send-keys "$BIN3"

tmux select-layout even-vertical

tmux bind -n C-q kill-session

tmux attach-session -t $SESSION_NAME

