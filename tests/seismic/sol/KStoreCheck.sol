contract KStoreCheck {
    function test() public {
        uint256 slot = 0x987654321;
        uint256 otherSlot = 0x1111;

        uint256 emptyResult;
        assembly {
            emptyResult := sload(slot)
        }

        uint256 expected = 0x123456789;
        uint256 result;
        uint256 otherResult;
        assembly {
            kstore(slot, expected)
            result := sload(slot)
            otherResult := sload(otherSlot)
        }

        assert(emptyResult == 0);
        assert(result == expected);
        assert(otherResult == 0);
    }
}
