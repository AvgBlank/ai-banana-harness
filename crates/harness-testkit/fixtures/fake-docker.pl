#!/usr/bin/env perl
# A stand-in for the `docker` command-line client, for tests that need no daemon. State lives in
# the `state` directory next to this script: `images` (one name per line), `flags/<name>` (failure switches), `log` (every
# call, arguments separated by \037), and `c/<container>/` (one directory per container).
# It really runs the container's command, locally and in a process group of its own, with an empty
# environment (as a container has), so tests can see output, exit codes, kills and leftovers.
use strict;
use warnings;
use File::Basename qw(dirname);
use POSIX qw(setsid WIFEXITED WEXITSTATUS WTERMSIG);

my $S = dirname($0) . "/state";
my @args = @ARGV;

open(my $log, '>>', "$S/log") or die $!;
print $log join("\037", @args), "\n";
close $log;

sub flag { return -e "$S/flags/$_[0]" }
sub slurp { my $f = shift; open(my $h, '<', $f) or return ''; local $/; my $t = <$h>; close $h; return defined $t ? $t : '' }
sub spew { my ($f, $t) = @_; open(my $h, '>', $f) or die "$f: $!"; print $h $t; close $h }
sub fail { my ($code, $msg) = @_; print STDERR "$msg\n"; exit $code }
sub nap { select(undef, undef, undef, $_[0]) }

my $cmd = shift(@args) // '';
if (flag('daemon_down')) {
    fail(1, 'Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?');
}

if ($cmd eq 'image' && ($args[0] // '') eq 'inspect') {
    my $image = $args[-1];
    my %have = map { chomp; ($_ => 1) } split /^/, slurp("$S/images");
    fail(1, "Error response from daemon: No such image: $image") unless $have{$image};
    print "sha256:fake-image-id\n";
    exit 0;
}

if ($cmd eq 'create') {
    my ($name, $wd, @labels);
    for (my $i = 0; $i < @args; $i++) {
        $name = $args[$i + 1] if $args[$i] eq '--name';
        $wd = $args[$i + 1] if $args[$i] eq '-w';
        push @labels, $args[$i + 1] if $args[$i] eq '--label';
    }
    my $command = $args[-1];
    nap(0.4) if flag('slow_create');
    if (flag('mount_denied')) {
        fail(125, "docker: Error response from daemon: invalid mount config for type \"bind\": bind source path does not exist: $wd");
    }
    mkdir "$S/c" unless -d "$S/c";
    mkdir "$S/c/$name" or fail(125, "Conflict. The container name \"$name\" is already in use");
    spew("$S/c/$name/cmd", $command);
    spew("$S/c/$name/wd", $wd // '/');
    spew("$S/c/$name/labels", join("\n", @labels));
    spew("$S/c/$name/state", "created 0 0");
    print "fake-id-$name\n";
    exit 0;
}

if ($cmd eq 'start') {
    my $name = $args[-1];
    fail(1, "Error: No such container: $name") unless -d "$S/c/$name";
    fail(125, "Error response from daemon: failed to create task for container: fake start failure") if flag('start_fails');
    if (flag('oom')) {
        spew("$S/c/$name/state", "exited 137 1");
        exit 137;
    }
    my $command = slurp("$S/c/$name/cmd");
    my $wd = slurp("$S/c/$name/wd");
    my $pid = fork();
    die "fork: $!" unless defined $pid;
    if ($pid == 0) {
        setsid();
        chdir $wd or exit 126;
        %ENV = (PATH => $ENV{PATH}, HOME => '/tmp');
        exec { 'bash' } 'bash', '-lc', $command or exit 127;
    }
    spew("$S/c/$name/pid", $pid);
    spew("$S/c/$name/state", "running 0 0");
    kill 'STOP', $$ if flag('client_stops');
    waitpid($pid, 0);
    my $status = $?;
    my $code = WIFEXITED($status) ? WEXITSTATUS($status) : 128 + WTERMSIG($status);
    spew("$S/c/$name/state", "exited $code 0");
    exit $code;
}

if ($cmd eq 'inspect') {
    my $name = $args[-1];
    fail(1, "Error response from daemon: No such object: $name") unless -d "$S/c/$name";
    my ($status, $code, $oom) = split ' ', slurp("$S/c/$name/state");
    print "$code ", ($oom ? 'true' : 'false'), " $status \n";
    exit 0;
}

if ($cmd eq 'kill') {
    my $name = $args[-1];
    fail(1, "Error response from daemon: No such container: $name") unless -d "$S/c/$name";
    my $pid = slurp("$S/c/$name/pid");
    my ($status) = split ' ', slurp("$S/c/$name/state");
    fail(1, "Error response from daemon: cannot kill container: $name: container $name is not running")
        unless $pid && $status eq 'running';
    kill 'KILL', -$pid;
    exit 0;
}

if ($cmd eq 'rm') {
    fail(1, 'Error response from daemon: removal of the container is already in progress') if flag('rm_fails');
    for my $name (grep { !/^-/ } @args) {
        my $dir = "$S/c/$name";
        next unless -d $dir;
        my $pid = slurp("$dir/pid");
        kill 'KILL', -$pid if $pid;
        unlink glob("$dir/*");
        rmdir $dir;
    }
    exit 0;
}

if ($cmd eq 'ps') {
    my ($filter) = map { $args[$_ + 1] } grep { $args[$_] eq '--filter' } 0 .. $#args;
    $filter =~ s/^label=// if defined $filter;
    my $format = join(' ', map { $args[$_ + 1] } grep { $args[$_] eq '--format' } 0 .. $#args);
    for my $dir (sort glob("$S/c/*")) {
        my ($name) = $dir =~ m{([^/]+)$};
        my $labels = slurp("$dir/labels");
        next if defined $filter && $labels !~ /\Q$filter\E/;
        if ($format =~ /\.Label/) {
            my ($owner) = $labels =~ /banana-harness\.pid=(\S+)/;
            print "$name ", defined $owner ? $owner : '', "\n";
        } else {
            print "$name\n";
        }
    }
    exit 0;
}

fail(1, "fake docker: unsupported command: $cmd");
