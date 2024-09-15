const main = () => {
    subscribe();
}

window.onload = main;

export const render = (channels) => {
    const root = document.getElementById("channels");

    let text = "";
    for (const name in channels) {
        const channel = channels[name]
        const date = seconds_to_date(channel.last.time);
        text += `<div><h3>${name}</h3>`
        text += `<div>updated: ${format_date(date)}</div>`
        text += `<div>value: <b>${channel.last.value}</b></div>`
        text += `<div>min: <b>${channel.min}</b></div>`
        text += `<div>max: <b>${channel.max}</b></div>`
        text += `<div>average: <b>${channel.mean}</b></div>`
        text += `</div>`
    }
    if (text.length === 0) {
        text = "<i>No channels</i>"
    }

    root.innerHTML = text;
}

const TIMEOUT = 10 * 1000;

const subscribe = () => {
    const xhttp = new XMLHttpRequest();
    xhttp.onload = function () {
        render(JSON.parse(this.responseText));
        setTimeout(subscribe, TIMEOUT);
    }
    xhttp.onerror = (e) => {
        console.error("Get error:", e);
        setTimeout(subscribe, TIMEOUT);
    };
    xhttp.open("GET", "../info", true);
    xhttp.send();
}

export const seconds_to_date = (seconds) => {
    let date = new Date(0);
    date.setUTCSeconds(seconds);
    return date;
}

export const format_date = (date) => {
    return ("0" + date.getDate()).slice(-2) + "-"
        + ("0" + (date.getMonth() + 1)).slice(-2) + "-"
        + date.getFullYear() + " "
        + ("0" + date.getHours()).slice(-2) + ":"
        + ("0" + date.getMinutes()).slice(-2);
}
