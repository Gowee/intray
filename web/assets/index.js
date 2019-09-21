// Size of chunks.
const CHUNK_SIZE = 4 * 1024 * 1024;
// Files of which the size is <= `ONESHOT_THRESHOLD` will be uploaded directly without chunking.
const ONESHOT_THRESHHOLD = CHUNK_SIZE;
// Number of workers, per which uploads one file at a time.
const CONCURRENT_WORKER = 3;
// Retry to upload chunks if failed for at most `CHUNK_RETRY` times;
const CHUNK_RETRY = 3;

const logBase = (base, number) => Math.log(number) / Math.log(base);
function size_to_readable(size) {
    if (size === 0) {
        return "0 B";
    }
    const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
    const level = Math.min(Math.floor(logBase(1024, size)), units.length - 1);
    const number = size / 1024 ** level;
    return `${number.toFixed(2)} ${units[level]}`;
}
function period_to_readable(period) {
    // ms
    period /= 1000;
    if (period < 0.01) {
        return "an instant";
    }
    let prefix = "";
    if (period > 24 * 3600) {
        const day = Math.floor(period / 24 * 3600 * 1000);
        prefix = `${day} ${(day > 1) ? "days" : "day"}`;
    }
    const units = ["sec", "min", "hr"];
    // \log_60{n} < 0 where n < 1 
    const level = period > 1 ? Math.min(Math.floor(logBase(60, period)), units.length - 1) : 0;
    const number = period / 60 ** level;
    // https://english.stackexchange.com/questions/193313/is-it-hour-or-hours-when-used-in-a-phrase
    const suffix = `${number.toFixed(2)} ${units[level]}${number === 1 ? "" : "s"}`;
    // assert suffix < 24 hrs
    return `${prefix} ${suffix}`;
    // TODO: time is different from size; maybe expected: ?h?m?s
}
function sleep(period) {
    return new Promise((resolve, _reject) => setTimeout(resolve, period));
}
// TODO: No support for IE
class Task {
    constructor(id, file) {
        this.id = id;
        this.file = file;
        this.progress_fn = null;
    }

    setProgress(progress) {
        this.progress_fn && this.progress_fn(progress)
    }

    setProgressFn(progress_fn) {
        this.progress_fn = progress_fn;
    }
}

const TASKS = { pending: new Array(), failed: new Map() };
const WORKERS = new Map();

const dropZone = document.getElementById("drop-zone");
const fileInput = document.getElementById("file-input");
const taskList = document.getElementById("task-list");
const fileNameList = document.getElementById("file-name-list");
const uploadButton = document.getElementById("upload-button");
const taskItemTemplate = document.getElementById("task-item-template");
// TODO: in EDGE: SCRIPT438: Object doesn't support property or method 'fromEntries'
const statusTemplate = Object.fromEntries(
    ["pending", "progress", "failed", "done"].map(
        status => [status, document.getElementById(`status-${status}-template`)]));
let filesSelected = null;
let taskId = 0;
let workerToken = 0;

dropZone.addEventListener("drop", function (event) {
    console.log("Dropped.");
    //const files = (new Array(event.dataTransfer.items)).filter(item => item.kind === "file").map(item => item.getAsFile());
    const files = event.dataTransfer.files;
    if (files.length === 0) {
        // alert("Only files are supported.");
        return;
    }
    event.preventDefault();
    onSelectFiles(files);
});
dropZone.addEventListener("dragover", function (event) {
    console.log("Dragging over.");
    event.preventDefault();
});
fileInput.addEventListener("change", () => onSelectFiles(fileInput.files));
uploadButton.addEventListener("click", onUpload);

function onSelectFiles(files) {
    // TODO: handle zero properly
    if (files.length === 0) {
        fileNameList.textContent = `No files selected.`;
        uploadButton.setAttribute("disabled", true);
    }
    else {
        const file_names = Array.from(files).map((file) => file.name).join(", ");
        fileNameList.innerHTML = `<small>${files.length} selected:</small> ${file_names} .`;
        uploadButton.removeAttribute("disabled");
        fileNameList.title = file_names;
        console.log(`File selected ${files.length}`);
        console.log(files);
    }
    filesSelected = files;
}

async function onUpload() {
    const files = filesSelected;
    for (const file of files) {
        taskId += 1;
        const taskItem = document.importNode(taskItemTemplate.content, true);
        taskItem.firstElementChild.setAttribute("id", `task-item-${taskId}`);
        taskItem.firstElementChild.dataset.id = taskId;
        console.log(file);
        const statusPending = document.importNode(statusTemplate.pending.content, true);
        taskItem.querySelector("[name=id]").textContent = taskId;
        taskItem.querySelector("[name=name]").textContent = file.name;
        taskItem.querySelector("[name=size]").textContent = size_to_readable(file.size);
        taskItem.querySelector("[name=status]").appendChild(statusPending);
        taskList.insertBefore(taskItem, taskList.firstElementChild);
        TASKS.pending.push(new Task(taskId, file));
    }
    // Clean selected files
    filesSelected = null;
    fileInput.value = "";
    fileInput.dispatchEvent(new Event("change"));
}

async function upload_worker(token) {
    console.log(`Worker [${token}] started.`);
    while (WORKERS.get(token) !== undefined) {
        if (TASKS.pending.length === 0) {
            await sleep(100);
        }
        else {
            const task = TASKS.pending.shift();
            console.log(`Worker [${token}] fetched task ${task.id} with a file named ${task.file.name}`);
            const statusProgress = document.importNode(statusTemplate.progress.content, true);
            const taskItem = document.getElementById(`task-item-${task.id}`);
            const statusContainer = taskItem.querySelector("[name=status]")
            const progressBar = statusProgress.querySelector("[name=progress]"); // Should before replaceChild
            statusContainer.replaceChild(statusProgress, statusContainer.firstElementChild);
            const progress_fn = function(progress) {
                progressBar.textContent = `${progress} %`;
                progressBar.setAttribute("value", progress);
            };
            task.setProgressFn(progress_fn);
            try {
                const result = await upload(task);
                const statusDone = document.importNode(statusTemplate.done.content, true);
                statusDone.querySelector("[name=elapsed]").textContent = period_to_readable(result);
                statusContainer.replaceChild(statusDone, statusContainer.firstElementChild);
            }
            catch (e) {
                const statusFailed = document.importNode(statusTemplate.failed.content, true);
                statusFailed.firstElementChild.title = e;
                statusContainer.replaceChild(statusFailed, statusContainer.firstElementChild);
            }
        }
    }
    console.log(`Worker [${token}] ends.`);
}

async function upload(task) {
    const file = task.file;
    const start_at = Date.now();
    const metadata = {
        'file_name': file.name,
        'file_size': file.size,
        'chunk_size': CHUNK_SIZE
    };
    console.log("Uploading", metadata);
    // Here treat `job` as initialized uploading instance of `task`.
    try {
        const job = await (await fetch("/upload/start", {
            method: "POST",
            headers: { 'Content-Type': "application/json" },
            body: JSON.stringify(metadata)
        })).json();
    if (job.ok) {
        const chunk_number = Math.ceil(file.size / CHUNK_SIZE);
        const file_token = job.file_token;
        for (let chunk_index = 0; chunk_index < chunk_number; chunk_index++) {
            let chunk_ok;
            for (let retry = 0; retry < CHUNK_RETRY; retry++) {
                try {
                    const chunk = await (await fetch(`/upload/${file_token}/${chunk_index}`, {
                        method: "POST",
                        headers: { 'Content-Type': "application/octet-stream" },
                        body: file.slice(chunk_index * CHUNK_SIZE, (chunk_index + 1) * CHUNK_SIZE)
                    })).json();
                    if (chunk.ok) {
                        task.setProgress((chunk_index + 1) / (chunk_number) * 100);
                        console.log(`Uploaded ${chunk_index + 1}/${chunk_number} chunks.`);
                        chunk_ok = true;
                        break;
                    }
                }
                catch (e) {
                    console.log(`Failed: ${e}, retrying.`);
                    throw e;
                }
            }
            if (chunk_ok !== true) {
                throw new Error(`Maximum retry times reached.`)
            }
        }
        try {
            const result = await (await fetch("/upload/finish", {
                'method': "POST",
                headers: { 'Content-Type': "application/json" },
                body: JSON.stringify({
                    file_token: file_token
                })
            })).json();
            if (result.ok) {
                const elapsed = Date.now() - start_at;
                console.log(`Successfully uploaded ${file.name} with \
                        ${file.size / 1024 / 1024} MiBs in ${elapsed / 1000} seconds at \
                        ${file.size / 1024 / 1024 / (elapsed / 1000)} MiB/sec.`);
            }
            else {
                throw new Error(`server error: ${result.error}`);
            }
        }
        catch (e) {
            throw new Error(`Error when finishing: ${e}`)
        }
    }
    else {
        throw new Error(`server error: ${job.error}`);
    }
    }
    catch (e) {
        throw new Error(`Error when initialzing: ${e}`);
    }
    const elapsed = Date.now() - start_at;
    return elapsed;
}

fileInput.dispatchEvent(new Event("change"));
for (let i = 0; i < CONCURRENT_WORKER; i++) {
    WORKERS.set(workerToken, null);
    WORKERS.set(workerToken, upload_worker(workerToken));
    workerToken += 1;
}