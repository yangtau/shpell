# x — natural language to shell commands (zsh integration)
#
# Press Tab on an empty line to enter X mode: an interactive prompt run by
# `x compose`, entirely outside zle. Type a request after the ❯ prompt; the
# generated command streams in after the ✻ icon, which pulses while generating.
# Then:
#   Enter (empty)   accept — back to zsh, the command lands on the prompt
#                   and runs; the whole exchange stays on screen above it
#   more text       refine the command with a follow-up request
#   e               back to zsh with the command on the prompt, NOT run
#   Ctrl-C / Ctrl-D cancel
#
# Because input and streaming happen inside `x compose`, zle never sees the
# natural-language text — no syntax-highlighting, history-expansion or PS2
# surprises. Icons: export X_USER_ICON / X_AI_ICON before use to override.

(( ${+_X_ZSH_LOADED} )) && return
typeset -g _X_ZSH_LOADED=1

# the model may answer non-command requests with a `# ...` comment line
setopt interactive_comments

typeset -g _x_tab_fallback=  # widget Tab was bound to before we took it over

_x_compose() {
  # only hijack Tab on an empty primary prompt; otherwise delegate to completion
  if [[ -n $BUFFER || $CONTEXT != start ]]; then
    zle "${_x_tab_fallback:-expand-or-complete}"
    return
  fi
  local out rc=1 ttysave
  zle -I   # x compose takes over drawing from here
  ttysave=$(stty -g < /dev/tty 2>/dev/null)
  {
    # zle keeps the tty raw during widgets; give x compose a cooked tty so
    # the terminal driver provides line editing and echo for the query
    stty sane < /dev/tty 2>/dev/null
    out=$(COLUMNS=$COLUMNS command x compose --shell zsh < /dev/tty 2> /dev/tty)
    rc=$?
  } always {
    [[ -n $ttysave ]] && stty "$ttysave" < /dev/tty 2>/dev/null
  }
  case $rc in
    0)   # accepted: put the command on the prompt and run it
      BUFFER=$out
      CURSOR=$#BUFFER
      zle reset-prompt
      [[ -n $out ]] && zle .accept-line
      ;;
    10)  # edit: leave the command on the prompt for review
      BUFFER=$out
      CURSOR=$#BUFFER
      zle reset-prompt
      ;;
    *)   zle reset-prompt ;;
  esac
}
zle -N _x_compose

# Claim Tab from precmd so we run after every other plugin (fzf-tab etc.) has
# bound it; remember the previous widget so non-empty lines still complete.
_x_install() {
  local km cur
  for km in main viins emacs; do
    cur=${${(z)$(bindkey -M $km '^I' 2>/dev/null)}[2]}
    [[ -n $cur && $cur != _x_compose ]] && _x_tab_fallback=$cur
    bindkey -M $km '^I' _x_compose 2>/dev/null
  done
  add-zsh-hook -d precmd _x_install
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd _x_install
