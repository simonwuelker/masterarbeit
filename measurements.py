import os
from os import listdir
from os.path import isfile, join
import subprocess
import json
import shutil

def problems(path):
    for filename in listdir(path):
        full_path = join(path, filename)
        if isfile(full_path):
            yield full_path

num_procs = os.cpu_count() // 8
TEMP_DIR = "temp"
MALLOB_BINARY = "~/mallob2/build/mallob"
PROBLEMS = "tooling/instances"
result = []

for index, problem in enumerate(problems(PROBLEMS)):
    # First, solve the problem
    print("Solving", problem)
    command = [
        "mpirun",
        "-np",
        str(num_procs),
        "--bind-to=core",
        "--map-by",
        f"ppr:{num_procs}:node:pe=4",
        str(MALLOB_BINARY),
        "-t=4",
        f"-mono={problem}",
        "-satsolver=c",
        "--palrup",
        f"-proof-dir={TEMP_DIR}",
    ]
    print("Invoking", command)

    env = os.environ.copy()
    env["RDMAV_FORK_SAFE"] = "1"
    env["NPROCS"] = str(num_procs)

    child = subprocess.Popen(
        command,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    try:
        child.wait()
    except Exception as exc:
        raise RuntimeError("Waiting for mallob to complete") from exc

    # Then, reduce the proof
    proof_dir = join(TEMP_DIR, [x for x in listdir(TEMP_DIR)][0])
    print("Reducing from", proof_dir)
    destination = join(TEMP_DIR, "stripped")
    cmd = ["cargo", "r", "-r", "--", "strip", f"{TEMP_DIR}", destination]
    print("Invoking", cmd)
    child = subprocess.Popen(
        cmd,
    )

    try:
        child.wait()
    except Exception as exc:
        raise RuntimeError("Waiting for proof stripping") from exc

    # Then, run the depth checker
    child = subprocess.Popen(
        ["cargo", "r", "-r", "--", "depth", destination],
    )

    # Read its output file
    with open("out.json", "r") as result_file:
        data = json.load(result_file)

    data["problem"] = problem

    result.append(data)

    print("Removing results")
    shutil.rmtree(TEMP_DIR)

    if len(result) > 9:
        break;

print("Writing result file")
with open("measurements.json", "w") as result_file:
    json.dump(result, result_file)
