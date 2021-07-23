import { AccountStatus, Notification } from "./json";

class ActionListerner {
    btn_execute_action: HTMLButtonElement;
    div_live_updates_info: HTMLElement;
    div_fully_verified_info: HTMLElement;

    div_display_name_overview: HTMLElement;
    display_name_value: HTMLElement;
    display_name_validity: HTMLElement;

    div_verification_overview: HTMLElement;
    verification_overview: HTMLElement;

    div_email_second_challenge: HTMLElement;
    email_second_challenge: HTMLElement;

    div_unsupported_overview: HTMLElement;
    unsupported_overview: HTMLElement;

    constructor() {
        // Register relevant elements.
        this.div_live_updates_info =
            document
                .getElementById("div-live-updates-info")! as HTMLButtonElement;

        this.div_fully_verified_info =
            document
                .getElementById("div-fully-verified-info")! as HTMLButtonElement;

        this.btn_execute_action =
            document
                .getElementById("execute-action")! as HTMLButtonElement;

        this.div_display_name_overview =
            document
                .getElementById("div-display-name-overview")!;

        this.display_name_value =
            document
                .getElementById("display-name-value")!;

        this.display_name_validity =
            document
                .getElementById("display-name-validity")!;

        this.div_verification_overview =
            document
                .getElementById("div-verification-overview")!;

        this.div_email_second_challenge =
            document
                .getElementById("div-email-second-challenge")!;

        this.email_second_challenge =
            document
                .getElementById("email-second-challenge")!;

        this.verification_overview =
            document
                .getElementById("verification-overview")!;

        this.div_unsupported_overview =
            document
                .getElementById("div-unsupported-overview")!;

        this.unsupported_overview =
            document
                .getElementById("unsupported-overview")!;

        // Handler for choosing network, e.g. "Kusama" or "Polkadot".
        document
            .getElementById("network-options")!
            .addEventListener("click", (e: Event) => {
                document
                    .getElementById("specify-network")!
                    .innerText = (e.target as HTMLAnchorElement).innerText;
            });

        // Handler for choosing action, e.g. "Request Judgement".
        document
            .getElementById("action-options")!
            .addEventListener("click", (e: Event) => {
                document
                    .getElementById("specify-action")!
                    .innerText = (e.target as HTMLAnchorElement).innerText;
            });

        // Handler for executing action and communicating with the backend API.
        this.btn_execute_action
            .addEventListener("click", (_: Event) => {
                window.location.href = "?network="
                    + (document.getElementById("specify-network")! as HTMLInputElement).innerHTML.toLowerCase()
                    + "&address="
                    + (document.getElementById("specify-address")! as HTMLInputElement).value;
            });

        let params = new URLSearchParams(window.location.search);
        let network = params.get("network");
        let address = params.get("address");
        console.log(network);
        console.log(address);
        if (network != null && address != null) {
            (document.getElementById("specify-network")! as HTMLInputElement).innerHTML = capitalizeFirstLetter(network);
            (document.getElementById("specify-address")! as HTMLInputElement).value = address;
            this.executeAction();
        }
    }
    executeAction() {
        this.btn_execute_action.disabled = true;

        const action = document.getElementById("specify-action")!.innerHTML;
        const address = (document.getElementById("specify-address")! as HTMLInputElement).value;
        const network = document.getElementById("specify-network")!.innerHTML.toLowerCase();

        this.btn_execute_action
            .innerHTML = `
                <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                <span class="visually-hidden"></span>
            `;

        if (action == "Request Judgement") {
            const socket = new WebSocket('ws://localhost:9000/api/account_status');

            socket.addEventListener("open", (_: Event) => {
                socket.send(JSON.stringify({ address: address, chain: network }));

                this.div_live_updates_info.classList.remove("invisible");
            });

            socket.addEventListener("message", (event: Event) => {
                let msg = (event as MessageEvent);
                console.log(msg.data);
                this.parseAccountStatus(msg);
            });
        }
    }
    parseAccountStatus(msg: MessageEvent) {
        let table = "";
        let unsupported = "";

        const parsed: AccountStatus = JSON.parse(msg.data);
        if (parsed.result_type == "ok") {
            // TODO: Check if 'fields` is empty.

            table += `
                <thead>
                    <tr>
                        <th scope="col">#</th>
                        <th scope="col">Type</th>
                        <th scope="col">Challenge</th>
                        <th scope="col">From</th>
                        <th scope="col">To</th>
                        <th scope="col">Status</th>
                    </tr>
                </thead>
                <tbody>
            `;

            let counter = 1;
            for (let field of parsed.message.state.fields) {
                if (field.challenge.challenge_type == "expected_message") {
                    let validity;
                    if (field.challenge.content.expected.is_verified) {
                        if (field.challenge.content.second && !field.challenge.content.second!.is_verified) {
                            validity = '<span class="badge bg-info">verified (1/2)</span>';

                            this.email_second_challenge.innerHTML = `${field.value.value}`;
                            this.div_email_second_challenge.classList.remove("invisible");
                        } else {
                            validity = '<span class="badge bg-success">verified</span>';

                            if (field.value.type == "email") {
                                this.div_email_second_challenge.classList.add("invisible");
                            }
                        }
                    } else {
                        validity = '<span class="badge bg-warning text-dark">unverified</span>';
                    }

                    table += `
                        <tr>
                            <th scope="row">${counter}</th>
                            <td>${capitalizeFirstLetter(field.value.type)}</td>
                            <td>${field.challenge.content.expected.value}</td>
                            <td>${field.value.value}</td>
                            <td>temp</td>
                            <td>${validity}</td>
                        </tr>
                    `;

                    counter += 1;
                } else if (field.challenge.challenge_type == "background_check" && field.value.type == "display_name") {
                    let elem = this.display_name_validity;

                    if (field.challenge.content.passed) {
                        elem.innerHTML = '<span class="badge bg-success">valid</span>';
                        elem.classList.add("text-success");
                    } else {
                        elem.innerHTML = '<span class="badge bg-danger">invalid</span>';
                        elem.classList.add("text-danger");
                    }

                    this.display_name_value.innerHTML = field.value.value;
                    this.div_display_name_overview.classList.remove("invisible");
                } else if (field.challenge.challenge_type == "unsupported") {
                    unsupported += `<li>${capitalizeFirstLetter(field.value.type)} ("${field.value.value}")</li>`;
                }
            }

            table += '</tbody>';

            this.verification_overview.innerHTML = table;
            this.div_verification_overview.classList.remove("invisible");

            if (unsupported.length != 0) {
                this.unsupported_overview.innerHTML = unsupported;
                this.div_unsupported_overview.classList.remove("invisible");
            } else {
                this.div_unsupported_overview.classList.add("invisible");
            }

            if (parsed.message.state.is_fully_verified) {
                this.div_fully_verified_info.innerHTML = `
                    <div class="row justify-content-center">
                        <div class="col-10 table-responsive bg-success p-2">
                            <h2 class="text-center text-white">Identity fully verified! ✔</h2>
                        </div>
                    </div>
                `;
            } else {
                this.div_fully_verified_info.innerHTML = '';
            }

            this.btn_execute_action
                .innerHTML = `
                    <div class="spinner-grow spinner-grow-sm" role="status">
                        <span class="visually-hidden"></span>
                    </div>
                `;
        } else if (parsed.result_type == "err") {
            this.btn_execute_action
                .innerHTML = `Go!`;
            this.btn_execute_action.disabled = false;
        } else {
            // Print unexpected error...
        }
    }
}

function capitalizeFirstLetter(word: string) {
    return (word.charAt(0).toUpperCase() + word.slice(1))
        .replace("_", " ");
}

function display_notification(notifications: Notification[]) {

}

new ActionListerner();
