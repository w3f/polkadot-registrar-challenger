import { AccountStatus, Notification } from "./json";

class ActionListerner {
    btn_execute_action: HTMLButtonElement;
    div_live_updates_info: HTMLElement;

    div_display_name_overview: HTMLElement;
    display_name_value: HTMLElement;
    display_name_validity: HTMLElement;

    div_verification_overview: HTMLElement;
    verification_overview: HTMLElement;

    div_unsupported_overview: HTMLElement;
    unsupported_overview: HTMLElement;

    constructor() {
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

        // Register relevant elements.
        this.div_live_updates_info =
            document
                .getElementById("div-live-updates-info")! as HTMLButtonElement;

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

        this.verification_overview =
            document
                .getElementById("verification-overview")!;

        this.div_unsupported_overview =
            document
                .getElementById("div-unsupported-overview")!;

        this.unsupported_overview =
            document
                .getElementById("unsupported-overview")!;

        // Handler for executing action and communicating with the backend API.
        this.btn_execute_action
            .addEventListener("click", (_: Event) => this.executeAction());
    }
    executeAction() {
        this.btn_execute_action.disabled = true;

        this.btn_execute_action
            .innerHTML = `
                <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                <span class="visually-hidden"></span>
            `;

        const action = document.getElementById("specify-action")!.innerHTML;
        const address = (document.getElementById("specify-address")! as HTMLInputElement).value;
        const network = document.getElementById("specify-network")!.innerHTML.toLowerCase();

        if (action == "Request Judgement") {
            const socket = new WebSocket('ws://161.35.213.120/api/account_status');

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
                        validity = '<span class="badge bg-success">verified</span>';
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

            this.unsupported_overview.innerHTML = unsupported;
            this.div_unsupported_overview.classList.remove("invisible");

            this.btn_execute_action
                .innerHTML = `
                    <div class="spinner-grow spinner-grow-sm" role="status">
                        <span class="visually-hidden"></span>
                    </div>
                `;
        } else if (parsed.result_type == "err") {

        } else {
            // Print unexpected error...
        }
    }
}

function capitalizeFirstLetter(word: string) {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

function display_notification(notifications: Notification[]) {

}

new ActionListerner();
