#!/usr/bin/python3

import matplotlib.pyplot as plt
import argparse
import numpy as np
import seaborn as sns

parser = argparse.ArgumentParser(prog="PalRup analyzer",
                    description="analyzes the output from the corresponding rust program",
                    epilog="u")

parser.add_argument("json", help="Path to JSON data", default="out.json")
parser.add_argument("--parse-edgelist", nargs="?", const="None", dest="parse_edgelist", help="Analyze the provided edgelist with graphvise")
parser.add_argument("--show-plots", action="store_true", help="Whether matplotlib plots should be shown or just saved")
args = parser.parse_args()

import json

with open(args.json, "r") as f:
    data = json.load(f)

import numpy as np
import matplotlib.pyplot as plt

def extract_problem_name(filename):
    without_hash = filename.split("-", 1)[1]
    without_hash = without_hash.removesuffix(".cnf.xz")
    without_hash = without_hash.removesuffix(".normalized")
    without_hash = without_hash.removesuffix(".sanitized")
    without_hash = without_hash.removesuffix(".normalised")
    return without_hash

def plot_import_generations(data):
    import_depths = np.array([data["per_file"][filename]["import_depths"] for filename in data["per_file"]])
    standard_deviation = np.std(import_depths, axis=0)
    mean = np.mean(import_depths, axis=0)

    fig, ax = plt.subplots()

    ax.bar(range(len(mean)), mean, yerr=standard_deviation, align='center')
    ax.set_ylabel('Percentage of total imports')
    ax.set_title('Maximum generations after import')
    ax.set_xlabel("Generations")
    ax.set_yscale("log")

    plt.savefig("import-generations.svg")

def plot_unused_imports_per_generation(data):
    fig, ax = plt.subplots(figsize=(12, 5), layout="constrained")
    for per_problem in data:
        id_of_unsat_clause = -1
        total_unused_imports = []
        problem_name = extract_problem_name(per_problem["problem_name"])

        # if not "oddball" in problem_name and not "at-least-two" in problem_name:
        #     pass
        print(per_problem["problem_name"], per_problem["time"])
        for file in per_problem["per_file"].values():
            if file["id_of_unsat_clause"] is not None:
                id_of_unsat_clause = max(file["id_of_unsat_clause"], id_of_unsat_clause)
            total_unused_imports += file["unused_imports"][::30]

        if len(total_unused_imports) == 0:
            continue
        print("Sorting...", len(total_unused_imports), id_of_unsat_clause)
        total_unused_imports.sort()

        ax.plot(
            [x / id_of_unsat_clause for x in total_unused_imports],
            [n / len(total_unused_imports) for n in range(len(total_unused_imports))],
            label =problem_name
        )

    ax.set_ylabel('Unused imports so far / total unused impor')
    ax.set_title('Frequency of unused imports throughout solving')
    ax.set_xlabel("Solving progress")
    leg = ax.legend(loc='center left', bbox_to_anchor=(1.0, 0.5))
    plt.savefig("unused-imports-per-communication-step.svg")

def plot_single_histogram(ax, title, histogram_data):
    ax.set_title(title + " size " + str(histogram_data["bucket_size"]))
    x = sorted(map(int, histogram_data["buckets"].keys()))
    y = np.array([histogram_data["buckets"][str(v)] for v in x], dtype=float)

    if np.max(y) / np.min(y) > 50:
        ax.set_yscale("log")
    ax.bar(x, y)

def plot_histograms(data):
    histogram_set = data["histogram_set"]

    fig, axs = plt.subplots(2, 3, constrained_layout = True)

    plot_single_histogram(axs[0, 0], "Incoming Edges", histogram_set["incoming_edges"])
    plot_single_histogram(axs[0, 1], "Outgoing Edges", histogram_set["outgoing_edges"])
    plot_single_histogram(axs[0, 2], "Number of literals", histogram_set["number_of_literals"])
    plot_single_histogram(axs[1, 0], "Clause ID", histogram_set["id"])
    plot_single_histogram(axs[1, 1], "Lifetime", histogram_set["lifetime"])

    plt.savefig("histograms.svg")

def plot_single_2d_histogram(ax, title, histogram_data, x_label, y_label):
    ax.set_title(title)
    ax.set_ylabel(y_label)
    ax.set_xlabel(x_label)

    # Setup appropriate formatters for x/y axis
    def x_axis_formatter(x, pos):
        return str(int(x*int(histogram_data["bucket_size"])))
    ax.xaxis.set_major_formatter(x_axis_formatter)

    def y_axis_formatter(x, pos):
        return str(int(x*int(histogram_data["inner_size"])))
    ax.yaxis.set_major_formatter(y_axis_formatter)

    buckets = histogram_data["buckets"]
    x_axis_keys = [int(x) for x in buckets.keys()]

    min_y = 2**32
    max_y = 0

    for row in buckets.values():
        min_y = min(min_y, min([int(x) for x in row["buckets"].keys()]))
        max_y = max(max_y, max([int(x) for x in row["buckets"].keys()]))

    if max_y > 1024:
        max_y = 1024 # FIXME: Support extreme out-of-range values
    rows = []
    for i in range(min(x_axis_keys), max(x_axis_keys) + 1):
        row_data = buckets[str(i)]["buckets"]
        # row_sum = sum(row_data.values())
        row_sum = 1.0
        row_data = [float(row_data.get(str(y), 0.0)) * 1000.0 for y in range(min_y, max_y)]
        rows.append(row_data)

    img = np.array(rows)
    ax.imshow(img.transpose())

def plot_2d_histograms(data):
    histogram_set = data["histogram_2d_set"]

    fig, axs = plt.subplots(2, 2, constrained_layout = True)

    plot_single_2d_histogram(
        axs[0, 0],
        "Number of literals over clause id",
        histogram_set["number_of_literals_over_clause_id"],
        "Clause ID",
        "#Literals"
    )
    plot_single_2d_histogram(
        axs[1, 0],
        "Lifetime over clause id",
        histogram_set["lifetime_over_clause_id"],
        "Clause ID",
        "Lifetime (in Clause IDs)"
    )
    plot_single_2d_histogram(
        axs[0, 1],
        "Minimum Lifetime over clause id",
        histogram_set["minimum_lifetime_over_clause_id"],
        "Clause ID",
        "Minimum Lifetime (in Clause IDs)"
    )
    plot_single_2d_histogram(
        axs[1, 1],
        "Minimum Lifetime over number of literals",
        histogram_set["minimum_lifetime_over_number_of_literals"],
        "# Literals",
        "Minimum Lifetime (in Clause IDs)"
    )

    plt.savefig("2d_histograms.svg")

def plot_important_clauses_over_time(data):
    importance_data = data["histogram_2d_set"]["importance_over_clause_id"]["buckets"]

    plt.title("Share of important clauses over time")
    print(importance_data)
    y_values = [histogram["buckets"].get("1") for histogram in importance_data.values()]
    plt.plot(importance_data.keys(), y_values)
    plt.savefig("importance_over_time.svg")

# https://stackoverflow.com/questions/13728392/moving-average-or-running-mean
def running_mean(x, N):
    cumsum = np.cumsum(np.insert(x, 0, 0))
    return (cumsum[N:] - cumsum[:-N]) / float(N)

def resample_1d(y, m):
    y = np.asarray(y)
    n = y.size

    if n == 0:
        return np.zeros(m)

    if m <= 0:
        raise ValueError("y must be non-empty and m must be > 0")
    if n == 1:
        return np.full(m, y[0], dtype=float)

    x_old = np.linspace(0, 1, n)   # original positions
    x_new = np.linspace(0, 1, m)   # target positions
    return np.interp(x_new, x_old, y)

def plot_share_of_important_clauses_per_thread_over_time(data):
    raw_data = data["critical_clauses_per_thread_over_time"]["buckets"]
    n_threads = len(data["critical_clauses_per_thread_over_time"]["buckets"])
    print(f"{n_threads} threads")

    GRANULARITY = data["stacked_plot_bucket_size"]
    def turn_to_percentages(data):
        longest_sequence = max(len(configuration) for configuration in data)
        x_axis_values = [GRANULARITY * i for i in range(longest_sequence)]

        # Compute sum per bucket
        sums = []
        i = 0
        while True:
            sum = 0
            one_thread_contributed = False
            for thread_id in range(len(data)):
                if len(data[thread_id]) > i:
                    sum += data[thread_id][i]
                    one_thread_contributed = True
            if sum == 0:
                sum = 1 # just avoids a division by zero
            sums.append(sum)
            i += 1
            if not one_thread_contributed:
                break


        y_axis_values = {}
        for thread_id in range(len(data)):
            thread_data = data[thread_id]
            padding_needed = longest_sequence - len(thread_data)
            padded_data = np.pad(thread_data, (0, padding_needed), mode="constant")
            assert len(padded_data) == longest_sequence
            y_axis_values[thread_id] = [padded_data[i] / sums[i] for i in range(longest_sequence)]

        return x_axis_values, y_axis_values, sums

    # Preprocess data such that it is aligned on imports

    print(GRANULARITY)

    import_epochs = data["imports_at_clause_ids"]

    processed_so_far = [0] * n_threads
    print(import_epochs[0]["lrat_ids"][0])
    reference_duration = import_epochs[0]["lrat_ids"][0] // GRANULARITY
    resampled_data = [np.array([])] * n_threads
    print(f"Determined that a import epoch should take around {reference_duration}px")

    xticks = [0]
    for index, import_epoch in enumerate(import_epochs):
        # We add 1 on the index here because we obviously don't communicate
        # at timestamp zero.
        xticks.append(reference_duration * (index + 1) * GRANULARITY)

        for thread_id in range(n_threads):
            data_for_thread = raw_data[thread_id]
            import_happened_at_x_value = import_epoch["lrat_ids"][thread_id] // GRANULARITY
            resampled = resample_1d(
                np.array(data_for_thread[processed_so_far[thread_id]:import_happened_at_x_value]),
                reference_duration
            )
            processed_so_far[thread_id] = import_happened_at_x_value
            resampled_data[thread_id] = np.concatenate([resampled_data[thread_id], resampled]);

    # Append any trailing data after the last import
    for thread_id in range(n_threads):
        data_for_thread = raw_data[thread_id]
        print("Trailing", len(data_for_thread[processed_so_far[thread_id]:]))
        # resampled_data[thread_id] = np.concatenate([resampled_data[thread_id], data_for_thread[processed_so_far[thread_id]:]]);
        # print(len(resampled_data[thread_id]))
        pass

    # Sum by configuration
    by_configuration = []
    for thread_id in range(n_threads):
        if thread_id < 10:
            by_configuration.append(resampled_data[thread_id])
        else:
            by_configuration[thread_id % 10] += resampled_data[thread_id]
    print(len(by_configuration))
    x_axis_values, y_axis_values, sums = turn_to_percentages(by_configuration)


    # fig, axs = plt.subplots(1, figsize = (10, 14))
    plt.stackplot(x_axis_values, y_axis_values.values(),
                labels=y_axis_values.keys(), alpha=0.8)
    plt.legend(loc='upper left', reverse=True)
    plt.title('Share of total clauses refutation DAG per configuration over time')
    plt.xlabel('Communication epoch')
    plt.ylabel('% of important clauses contributed')
    plt.legend().remove();
    x_labels = [str(i) for i in range(len(xticks))]
    plt.xticks(xticks, labels=x_labels)
    plt.savefig("share_of_important_clauses_over_time.svg")
    return
    print("xticks", xticks)

    # print([len(y_axis_values[t]) for t in range(n_threads)])
    # for import_epoch in import_epochs:
    #     for thread_id in range(n_threads):
    #         import_epoch_at = import_epoch["lrat_ids"][thread_id]
    #         import_happened_at_x_value = import_epoch_at // GRANULARITY
    #         if import_happened_at_x_value >= len(y_axis_values[0]):
    #             continue
    #         print(f"import at {import_happened_at_x_value} for thread {thread_id}, len is {len(y_axis_values[0])}")
    #         y = sum(y_axis_values[t][import_happened_at_x_value] for t in range(thread_id))
    #         axs[0].plot(import_epoch_at, y, marker='o', color='red', ms=10, zorder=10)
    #         print(import_happened_at_x_value, y)

    N = 16
    axs[1].set_title(f'Absolute number of important clauses over buckets (Avg over last {N})')
    sums = sums[:-1]
    sums = [sums[0]] * (N - 1) + sums
    axs[1].plot(x_axis_values, running_mean(np.array(sums), N))
    axs[1].set_xlabel('Clause ID')
    axs[1].set_ylabel('# Important clauses')

    x_axis_values, y_axis_values, sums = turn_to_percentages(data["imported_by_thread_over_time"]["buckets"])
    axs[2].stackplot(x_axis_values, y_axis_values.values(),
                labels=y_axis_values.keys(), alpha=0.8)
    axs[2].legend(loc='upper left', reverse=True)
    axs[2].set_title('Clauses imported from thread over time')
    axs[2].set_xlabel('Clause ID')
    axs[2].set_ylabel('% of important clauses contributed by this thread')
    axs[2].get_legend().remove();

    N = 16
    axs[3].set_title(f'Absolute number of important imports over buckets (Avg over last {N})')
    sums = sums[:-1]
    sums = [sums[0]] * (N - 1) + sums
    axs[3].plot(x_axis_values, running_mean(np.array(sums), N))
    axs[3].set_xlabel('Clause ID')
    axs[3].set_ylabel('# Important imports')

    axs[4].set_title('Average Lookbehind')
    x_axis_values, y_axis_values, sums = turn_to_percentages(data["average_lookbehind_per_bucket"]["buckets"])
    sums = sums[:-1]
    # sums = [sums[0]] * (N - 1) + sums
    axs[4].plot(x_axis_values, sums)
    axs[4].set_xlabel('Clause ID')
    axs[4].set_ylabel('Average lookbehind')

    plt.tight_layout()
    plt.savefig("share_of_important_clauses_over_time.svg")

def plot_contributions_over_time(data):
    data = data["critical_clauses_per_thread_over_time"]["buckets"]
    n_threads = len(data)

    x = [x for x in range(n_threads)]
    y = []
    colors = [plt.cm.tab10.colors[i % 10] for i in range(n_threads)]
    for n in range(n_threads):
        y.append(sum(data[n]))
    plt.title("Total contributions per thread across 10 configurations")
    plt.bar(x, y, color=colors)
    plt.xticks(x)
    plt.tight_layout();

def plot_contributions_over_time_single(data):
    contributions_per_configuration = [0] * 10
    print(data.keys())
    n_threads = 0
    for data in data["single_results"]:
        data = data["critical_clauses_per_thread_over_time"]["buckets"]
        n_threads = len(data)

        for n in range((n_threads // 10) * 10):
            contributions_per_configuration[n % 10] += sum(data[n])
    colors = [plt.cm.tab10.colors[i] for i in range(10)]
    contributions = [x / sum(contributions_per_configuration) for x in contributions_per_configuration]
    plt.title("Share of contributions by each configurations")
    plt.bar([x for x in range(10)], contributions,color=colors)
    plt.xticks([x for x in range(10)])
    plt.hlines(y=[0.1], xmin = 0, xmax = 10, colors=['g'], linestyles=['--'])
    plt.savefig("share_of_contributions_by_each_configuration.svg")
    plt.xlabel("Configuration ID")
    plt.ylabel("Share of refutation DAG")
    plt.tight_layout();

def rolling_window(a, window):
    shape = a.shape[:-1] + (a.shape[-1] - window + 1, window)
    strides = a.strides + (a.strides[-1],)
    return np.lib.stride_tricks.as_strided(a, shape=shape, strides=strides)

def plot_clause_depths_single(data):
    window_size = 4
    sums = np.array([sum(data[0])] * (window_size - 1) + [sum(at_depth) for at_depth in data])
    plt.plot(np.arange(0, len(data)), [min(window) for window in rolling_window(sums, window_size)])
    plt.grid()
    plt.yscale("log")
    plt.xlabel("Refutation Depth")
    plt.ylabel("Number of Clauses")
    plt.title("Breadth of the Refutation DAG")
    plt.tight_layout();
    plt.savefig("refutation_dag_breadth.svg")

def plot_clause_depths(data):
    fig, axs = plt.subplots(2, sharex=True, gridspec_kw={'height_ratios': [1, 3]})

    # Thread ratio per depth
    percentages = []
    for index, at_depth in enumerate(data):
        total = sum(at_depth)
        assert total != 0, f"No important clauses at depth {index}"
        percentages.append([x / total for x in at_depth])

    axs[1].stackplot(np.arange(0, len(data)), np.array(percentages).transpose())
    axs[0].set_title("Clause depths")
    axs[1].set_xlabel("Refutation Depth")
    # Absolute count per depth

    window_size = 4
    sums = np.array([sum(data[0])] * (window_size - 1) + [sum(at_depth) for at_depth in data])
    axs[0].plot(np.arange(0, len(data)), [min(window) for window in rolling_window(sums, window_size)])
    axs[0].grid()
    axs[0].set_yscale("log")
    plt.tight_layout();
    plt.savefig("clause_depths.svg")

# https://numpy.org/doc/stable/reference/generated/numpy.linalg.lstsq.html
def linreg(x, y):
    A = np.vstack([x, np.ones(len(x))]).T
    return np.linalg.lstsq(A, y)[0]


def plot_depth_volume_time_correlations(data):
    print(data[3].keys())
    data = sorted(data, key=lambda single_result: single_result["volume"])
    x = np.array([single_result["volume"] for single_result in data])
    y = np.array([single_result["time"] for single_result in data])
    print(len(x))
    plt.scatter(x, y, marker="x", label="Observed Data")
    plt.ylabel("time (seconds)")
    plt.xlabel("volume (clauses)")

    a, b = linreg(x, y)
    plt.plot(x, a*x + b, 'r', label='Fitted line')
    plt.legend()
    plt.yscale("log")
    plt.xscale("log")

    for i in range(len(data)):
        plt.annotate(extract_problem_name(data[i]["problem"]), (x[i], y[i]), ha="center", va="bottom")
    plt.savefig("depth_volume_time.svg")

def violin_plot(data):
    print(data["single_results"][0]["metric_store"].keys())

    fig, axs = plt.subplots(nrows=2, ncols=2)

    def for_metric(ax, name, ylabel, skip_greater_than=None, skip_value=None):
        when_critical = []
        when_not_critical = []
        skipped = 0
        for problem_data in data["single_results"]:
            metric_store = problem_data["metric_store"]
            for important, value in zip(metric_store["is_critical"], metric_store[name]):
                if skip_greater_than is not None and value > skip_greater_than:
                    if important:
                        skipped += 1
                    continue;
                if skip_value is not None and value == skip_value:
                    continue
                if important:
                    when_critical.append(value)
                else:
                    when_not_critical.append(value)


        total = when_critical + when_not_critical
        print(min(when_critical), min(when_not_critical))
        print("skipped important", skipped, "left with", len(when_critical))
        # axs.set_yscale("log")
        print(when_critical.count(3) / len(when_critical), when_not_critical.count(3) / len(when_not_critical))
        sns.histplot(discrete=True, data=when_critical, ax=ax, label="Important",stat="probability",)
        sns.histplot(discrete=True, data=when_not_critical, ax=ax, label="Not important",stat="probability")
        # ax.violinplot([when_not_critical, when_critical, total], showmeans=True)
        # ax.set_xticks([1, 2, 3], labels=["Not Important","Important",  "Total"])
        ax.set_xlabel(ylabel)
        ax.set_ylabel("Density")
        if max(total) > 10:
            ax.set_xticks(range(0, max(total)  +1, 5))
        else:
            ax.set_xticks(range(0, max(total)  +1))
        ax.grid(axis="y")

        ax.legend()

    for_metric(axs[0][0], "number_of_literals", "#Literals", skip_greater_than=25, skip_value=0)
    for_metric(axs[1][0], "incoming_edges", "#Hints", skip_greater_than = 10)
    for_metric(axs[0][1], "outgoing_edges", "#Derivations", skip_greater_than=7)
    fig.delaxes(axs[1][1])
    # for_metric(axs[1][1], "average_lookbehind", "Min Lookbehind", skip_greater_than=50000)
    plt.tight_layout()
    plt.savefig("violin_distribution.svg")

def plot_runtime_differences():
    with open("jsons/timing-normal-run-cook.json", "r") as outfile:
        data = json.load(outfile)
        expected_order = [problem["problem_name"] for problem in data["single_results"]]
        normal_timings = [problem["time"] for problem in data["single_results"]]
    with open("jsons/timing-searchonly-run-cook.json", "r") as outfile:
        data = json.load(outfile)
        order = [problem["problem_name"] for problem in data["single_results"]]
        assert expected_order == order
        searchonly_timings = [problem["time"] for problem in data["single_results"]]
    with open("jsons/timing-unsatonly-run-cook.json", "r") as outfile:
        data = json.load(outfile)
        order = [problem["problem_name"] for problem in data["single_results"]]
        assert expected_order == order
        unsat_timings = [problem["time"] for problem in data["single_results"]]

    fig = plt.figure(layout='constrained', figsize=(12, 5))
    gs = fig.add_gridspec(1, 2, width_ratios=[3, 2])
    ax = [fig.add_subplot(gs[0, 0]), fig.add_subplot(gs[0, 1])]
    # ax.set_xticks([x for x in range(len(expected_order))], labels=expected_order, rotation=340)
    ax[0].set_title("Solve time with different configurations")
    ax[0].set_ylabel("Time (seconds)")
    ax[0].set_xlabel("Problem ID")
    ax[0].set_yscale("log")
    ax[0].grid()

    problems = [extract_problem_name(p) for p in expected_order]
    ax[0].grouped_bar(
        [normal_timings, searchonly_timings, unsat_timings],
        tick_labels=[x for x in range(len(expected_order))],
        labels=["Default Portfolio", "Searchonly", "Unsat Preset"]
    )
    ax[0].legend()

    # === Table ===
    ax[1].axis("off")
    table_data = [
        [index, extract_problem_name(label)]
        for index, label in enumerate(expected_order)
    ]

    table = ax[1].table(
        cellText=table_data,
        colLabels=["#", "Category"],
        loc="center",
        cellLoc="left",
        colWidths=[0.1, 0.9],
    )

    table.auto_set_font_size(False)
    table.set_fontsize(9)
    table.scale(1, 1.8)
    for cell in table.get_celld().values():
        cell.PAD = 0.1
        pass
    # for container in res.bar_containers:
    #     ax.bar_label(container, padding=3)
    plt.savefig("runtime-differences.svg")

# plot_runtime_differences()
# plot_import_generations(data)
# plot_unused_imports_per_generation(data["single_results"])
# print(data["hostname"])
# for problem in data["single_results"]:
#     print(problem["problem_name"], problem["time"])
violin_plot(data)
# plot_histograms(data)
# plot_2d_histograms(data)
# plot_share_of_important_clauses_per_thread_over_time(data["single_results"][4])
# plot_share_of_important_clauses_per_thread_over_time(data)


# plot_share_of_important_clauses_per_thread_over_time(data["single_results"][3]) # good
# plot_share_of_important_clauses_per_thread_over_time(data["single_results"][5]) # nice
# plot_share_of_important_clauses_per_thread_over_time(data["single_results"][6]) # wtf
# plot_share_of_important_clauses_per_thread_over_time(data["single_results"][8]) # long time no progress then everything at once

# plot_share_of_important_clauses_per_thread_over_time(data)
# plot_contributions_over_time(data)
# plot_clause_depths(data)
# plot_contributions_over_time_single(data)
# plot_depth_volume_time_correlations(data)
if args.show_plots:
    plt.show()
