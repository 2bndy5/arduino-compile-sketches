
export def --wrapped find-sub-cmd [...cmd: string] {
    mut index = if ($cmd | first) == "uvx" {
        1
    } else if ($cmd | first 2) == ["uv", "run"] {
        2
    } else {
        0
    }
    if ($index == 0) {
        ($cmd | first 2 | str join ' ')
    } else {
        mut skip_val = false
        for arg in ($cmd | skip $index) {
            if ($arg | str starts-with "-") {
                # print $"Skipping arg: ($arg)"
                $skip_val = true
            } else if $skip_val {
                # print $"Skipping arg: ($arg)"
                $skip_val = false
            } else {
                # print $"Stopping at arg: ($arg)"
                break
            }
            $index = $index + 1
        }
        let sub_cmd = ($cmd | skip $index | first)
        $sub_cmd
    }
}

# A helper function to display the command being executed.
#
# This also prints the elapsed time the command took to execute.
export def --wrapped run-cmd [...cmd: string] {
    let app = if (
        ($cmd | first) == "cargo"
        or ($cmd | first) == "yarn"
        or ($cmd | first) == 'git'
        or ($cmd | first) == 'gh'
    ) {
        ($cmd | first 2) | str join ' '
    } else if (($cmd | first) in ["uv", "uvx"]) {
        find-sub-cmd ...$cmd
    } else {
        ($cmd | first)
    }
    print $"(ansi blue)\nRunning(ansi reset) ($cmd | str join ' ')"
    # spell-checker: disable-next-line
    let elapsed = timeit {|| ^($cmd | first) ...($cmd | skip 1)}
    print $"(ansi magenta)($app) took ($elapsed)(ansi reset)"
}
