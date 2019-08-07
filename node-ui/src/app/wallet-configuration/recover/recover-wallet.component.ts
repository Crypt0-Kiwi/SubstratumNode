// Copyright (c) 2017-2019, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.

import {Component, NgZone, OnInit} from '@angular/core';
import {WalletService} from '../../wallet.service';
import {Router} from '@angular/router';
import {FormControl, FormGroup, Validators} from '@angular/forms';
import {hardenedPathValidator, mnemonicValidator, mnemonicWordLengthValidator} from '../wallet.validator';
import {ConfigService} from '../../config.service';
import {wordlists} from '../wordlists';

@Component({
  selector: 'app-recover-wallet',
  templateUrl: './recover-wallet.component.html',
  styleUrls: ['./recover-wallet.component.scss']
})
export class RecoverWalletComponent implements OnInit {

  constructor(private configService: ConfigService,
              private walletService: WalletService,
              private router: Router,
              private ngZone: NgZone) {
  }

  publicAddress = '';
  errorText = '';
  wordlists = wordlists;

  walletConfig = new FormGroup({
      derivationPath: new FormControl('m/44\'/60\'/0\'/0/0', [Validators.required, hardenedPathValidator]),
      mnemonicPhrase: new FormControl('', [Validators.required, mnemonicWordLengthValidator]),
      mnemonicPassphrase: new FormControl('', []),
      wordlist: new FormControl('en', [Validators.required]),
      password: new FormControl('', [Validators.required])
    },
    {asyncValidators: [mnemonicValidator(this.walletService)]});

  ngOnInit() {
    this.walletConfig.controls['mnemonicPassphrase'].valueChanges
      .subscribe(() => this.generatePublicAddress());
    this.walletConfig.controls['derivationPath'].valueChanges
      .subscribe(() => this.generatePublicAddress());
    this.walletConfig.controls['mnemonicPhrase'].valueChanges
      .subscribe(() => this.generatePublicAddress());

    this.walletService.addressResponse.subscribe((address) => {
      this.ngZone.run(() => {
        this.publicAddress = address;
      });
    });

    this.walletService.recoverConsumingWalletResponse.subscribe((response: string) => {
      this.ngZone.run(() => {
          if (response === 'success') {
            this.configService.setEarningWallet(this.publicAddress);
            this.router.navigate(['/index']);
          } else {
            this.errorText = response;
          }
        }
      );
    });
  }

  generatePublicAddress() {
    const walletConfig = this.walletConfig;
    this.walletService.calculateAddress(
      walletConfig.get('mnemonicPhrase').value,
      walletConfig.get('derivationPath').value,
      walletConfig.get('mnemonicPassphrase').value,
      walletConfig.get('wordlist').value
    );
  }

  recover() {
    const walletConfig = this.walletConfig.value;
    const wordList = wordlists.find(wl => wl.value === walletConfig.wordlist).viewValue;
    this.walletService.recoverConsumingWallet(
      walletConfig.mnemonicPhrase, walletConfig.mnemonicPassphrase, walletConfig.derivationPath, wordList, walletConfig.password
    );
  }
}
